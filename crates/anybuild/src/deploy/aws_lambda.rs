//! AWS Lambda container-image deployment adapter.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use anyhow::{bail, Context, Result};

use crate::artifact::{ArtifactKind, RuntimeArtifact};
use crate::deploy::Deployer;
use crate::operation::OperationContext;
use crate::sdk::{AwsLambdaOptions, DeployOutcome, DeployTarget, LambdaArchitecture};

const LAMBDA_ADAPTER_IMAGE: &str = "public.ecr.aws/awsguru/aws-lambda-adapter:1.0.0";

pub(crate) struct AwsLambdaDeployer {
    options: AwsLambdaOptions,
    operation: OperationContext,
}

impl AwsLambdaDeployer {
    pub(crate) fn new(options: AwsLambdaOptions, operation: OperationContext) -> Self {
        Self { options, operation }
    }

    fn aws_binary(&self) -> &str {
        self.options.binary.as_deref().unwrap_or("aws")
    }

    fn docker_binary(&self) -> &str {
        self.options.docker_binary.as_deref().unwrap_or("docker")
    }

    fn profile_args(&self) -> Vec<String> {
        self.options
            .profile
            .as_deref()
            .filter(|profile| !profile.is_empty())
            .map(|profile| vec!["--profile".to_owned(), profile.to_owned()])
            .unwrap_or_default()
    }

    fn aws_args(&self, service: &str, action: &str, region: &str, args: &[&str]) -> Vec<String> {
        let mut command = vec![service.to_owned(), action.to_owned()];
        command.extend(args.iter().map(|arg| (*arg).to_owned()));
        command.extend(["--region".to_owned(), region.to_owned()]);
        command.extend(self.profile_args());
        command.push("--no-cli-pager".to_owned());
        command
    }

    fn capture(&self, command: &str, args: &[String], input: Option<&[u8]>) -> Result<Output> {
        let mut process = Command::new(command);
        self.operation.prepare_command(&mut process);
        process.args(args);
        if let Some(input) = input {
            process.stdin(Stdio::piped());
            process.stdout(Stdio::piped()).stderr(Stdio::piped());
            let mut child = process
                .spawn()
                .with_context(|| format!("failed to run {command}"))?;
            child
                .stdin
                .take()
                .expect("stdin is piped")
                .write_all(input)?;
            return child
                .wait_with_output()
                .with_context(|| format!("failed to run {command}"));
        }
        process
            .output()
            .with_context(|| format!("failed to run {command}"))
    }

    fn run(&self, command: &str, args: &[String]) -> Result<()> {
        let mut process = Command::new(command);
        self.operation.prepare_command(&mut process);
        process.args(args);
        let status = self
            .operation
            .command_status(&mut process)
            .with_context(|| format!("failed to run {command}"))?;
        anyhow::ensure!(
            status.success(),
            "Command {command} failed with exit code {:?}",
            status.code()
        );
        Ok(())
    }

    fn run_with_stdin(&self, command: &str, args: &[String], input: &[u8]) -> Result<()> {
        let mut process = Command::new(command);
        self.operation.prepare_command(&mut process);
        process.args(args);
        let status = self
            .operation
            .command_status_with_stdin(&mut process, input)
            .with_context(|| format!("failed to run {command}"))?;
        anyhow::ensure!(
            status.success(),
            "Command {command} failed with exit code {:?}",
            status.code()
        );
        Ok(())
    }

    fn output_text(output: &Output, description: &str) -> Result<String> {
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            let detail = if stderr.is_empty() { stdout } else { stderr };
            bail!("{description} failed: {detail}");
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    fn resolve_region(&self) -> Result<String> {
        if let Some(region) = self
            .options
            .region
            .as_deref()
            .filter(|region| !region.is_empty())
        {
            return Ok(region.to_owned());
        }
        for name in ["AWS_REGION", "AWS_DEFAULT_REGION"] {
            if let Some(region) = self
                .operation
                .environment_var(name)
                .or_else(|| std::env::var(name).ok())
                .filter(|region| !region.is_empty())
            {
                return Ok(region);
            }
        }

        let mut args = vec![
            "configure".to_owned(),
            "get".to_owned(),
            "region".to_owned(),
        ];
        args.extend(self.profile_args());
        args.push("--no-cli-pager".to_owned());
        let binary = self.aws_binary();
        let output = self.capture(binary, &args, None)?;
        let region = Self::output_text(&output, "resolving the AWS region")?;
        anyhow::ensure!(
            !region.is_empty(),
            "AWS region is not configured; pass --aws-region"
        );
        Ok(region)
    }

    fn ensure_repository(&self, repository: &str, region: &str) -> Result<String> {
        let describe = self.aws_args(
            "ecr",
            "describe-repositories",
            region,
            &[
                "--repository-names",
                repository,
                "--query",
                "repositories[0].repositoryUri",
                "--output",
                "text",
            ],
        );
        let binary = self.aws_binary();
        let output = self.capture(binary, &describe, None)?;
        if output.status.success() {
            let uri = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            anyhow::ensure!(
                !uri.is_empty() && uri != "None",
                "AWS returned an empty ECR URI"
            );
            return Ok(uri);
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("RepositoryNotFoundException") {
            return Self::output_text(&output, "describing the ECR repository");
        }

        let create = self.aws_args(
            "ecr",
            "create-repository",
            region,
            &[
                "--repository-name",
                repository,
                "--image-scanning-configuration",
                "scanOnPush=true",
                "--query",
                "repository.repositoryUri",
                "--output",
                "text",
            ],
        );
        let output = self.capture(binary, &create, None)?;
        let uri = Self::output_text(&output, "creating the ECR repository")?;
        anyhow::ensure!(
            !uri.is_empty() && uri != "None",
            "AWS returned an empty ECR URI"
        );
        Ok(uri)
    }

    fn push_image(&self, local_image: &str, repository_uri: &str, region: &str) -> Result<String> {
        let registry = repository_uri
            .split_once('/')
            .map(|(registry, _)| registry)
            .context("AWS returned an invalid ECR repository URI")?;
        let tag = self
            .options
            .image_tag
            .as_deref()
            .filter(|tag| !tag.is_empty())
            .unwrap_or("anybuild");
        let image_uri = format!("{repository_uri}:{tag}");

        let password_args = self.aws_args("ecr", "get-login-password", region, &[]);
        let aws_binary = self.aws_binary();
        let password_output = self.capture(aws_binary, &password_args, None)?;
        let password = Self::output_text(&password_output, "authenticating with Amazon ECR")?;
        let docker_binary = self.docker_binary();
        self.run_with_stdin(
            docker_binary,
            &[
                "login".to_owned(),
                "--username".to_owned(),
                "AWS".to_owned(),
                "--password-stdin".to_owned(),
                registry.to_owned(),
            ],
            password.as_bytes(),
        )?;
        self.run(
            docker_binary,
            &["tag".to_owned(), local_image.to_owned(), image_uri.clone()],
        )?;
        self.run(docker_binary, &["push".to_owned(), image_uri.clone()])?;
        Ok(image_uri)
    }

    fn function_exists(&self, function: &str, region: &str) -> Result<bool> {
        let args = self.aws_args(
            "lambda",
            "get-function",
            region,
            &["--function-name", function, "--output", "json"],
        );
        let output = self.capture(self.aws_binary(), &args, None)?;
        if output.status.success() {
            return Ok(true);
        }
        if String::from_utf8_lossy(&output.stderr).contains("ResourceNotFoundException") {
            return Ok(false);
        }
        Self::output_text(&output, "looking up the Lambda function")?;
        unreachable!()
    }

    fn publish_function(
        &self,
        function: &str,
        image_uri: &str,
        region: &str,
        architecture: LambdaArchitecture,
    ) -> Result<()> {
        let architecture = architecture.as_aws_value();
        let args = if self.function_exists(function, region)? {
            self.aws_args(
                "lambda",
                "update-function-code",
                region,
                &[
                    "--function-name",
                    function,
                    "--image-uri",
                    image_uri,
                    "--architectures",
                    architecture,
                ],
            )
        } else {
            let role = self
                .options
                .role
                .as_deref()
                .filter(|role| !role.is_empty())
                .context(
                    "Creating an AWS Lambda function requires --aws-role with an IAM role ARN",
                )?;
            self.aws_args(
                "lambda",
                "create-function",
                region,
                &[
                    "--function-name",
                    function,
                    "--package-type",
                    "Image",
                    "--code",
                    &format!("ImageUri={image_uri}"),
                    "--role",
                    role,
                    "--architectures",
                    architecture,
                ],
            )
        };
        self.run(self.aws_binary(), &args)
    }

    fn architecture(&self, artifact: &RuntimeArtifact) -> Result<LambdaArchitecture> {
        let artifact_architecture = match artifact.docker_platform() {
            Some("linux/amd64") => LambdaArchitecture::X86_64,
            Some("linux/arm64") => LambdaArchitecture::Arm64,
            Some(platform) => bail!("Unsupported Docker platform for AWS Lambda: {platform}"),
            None if std::env::consts::ARCH == "aarch64" => LambdaArchitecture::Arm64,
            None => LambdaArchitecture::X86_64,
        };
        if let Some(requested) = self.options.architecture {
            anyhow::ensure!(
                requested == artifact_architecture,
                "AWS Lambda architecture {} does not match the Docker artifact architecture {}",
                requested.as_aws_value(),
                artifact_architecture.as_aws_value()
            );
        }
        Ok(self.options.architecture.unwrap_or(artifact_architecture))
    }
}

impl Deployer for AwsLambdaDeployer {
    fn platform_name(&self) -> &'static str {
        "AWS Lambda"
    }

    fn artifact_kind(&self) -> ArtifactKind {
        ArtifactKind::Docker
    }

    fn load_legacy_artifact(&self, _anybuild_dir: &Path) -> Result<RuntimeArtifact> {
        bail!("AWS Lambda deployment requires a Docker artifact; build with --runner=docker first")
    }

    fn deploy(
        &mut self,
        artifact: &RuntimeArtifact,
        target: DeployTarget,
    ) -> Result<DeployOutcome> {
        if matches!(target, DeployTarget::WriteConfig { .. }) {
            bail!("AWS Lambda does not support Wasmer deployment configs");
        }
        let (artifact_dir, local_image, _context) = artifact.docker_parts().with_context(|| {
            format!(
                "{} deployment requires a Docker artifact, found {:?}",
                self.platform_name(),
                artifact.kind()
            )
        })?;
        let dockerfile = std::fs::read_to_string(artifact_dir.join("Dockerfile"))
            .with_context(|| "Docker artifact metadata is missing; rebuild with --runner=docker")?;
        anyhow::ensure!(
            dockerfile.contains(LAMBDA_ADAPTER_IMAGE),
            "Docker artifact predates AWS Lambda support; rebuild with --runner=docker"
        );

        let function = self
            .options
            .function
            .as_deref()
            .filter(|function| !function.is_empty())
            .context("AWS Lambda deployment requires --aws-function")?;
        let repository = self
            .options
            .repository
            .as_deref()
            .filter(|repository| !repository.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| repository_name(function));
        let region = self.resolve_region()?;
        let architecture = self.architecture(artifact)?;
        let repository_uri = self.ensure_repository(&repository, &region)?;
        let image_uri = self.push_image(local_image, &repository_uri, &region)?;
        self.publish_function(function, &image_uri, &region, architecture)?;

        Ok(DeployOutcome::Published {
            owner: None,
            name: Some(function.to_owned()),
        })
    }
}

fn repository_name(function: &str) -> String {
    let normalized: String = function
        .chars()
        .map(|character| {
            let character = character.to_ascii_lowercase();
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect();
    let normalized = normalized.trim_matches(['-', '_', '.']);
    if normalized.len() < 2 {
        "anybuild-lambda".to_owned()
    } else {
        normalized.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_names_are_ecr_compatible() {
        assert_eq!(repository_name("Acme API/Prod"), "acme-api-prod");
        assert_eq!(repository_name("!"), "anybuild-lambda");
    }

    #[test]
    fn aws_arguments_include_region_profile_and_disable_the_pager() {
        let deployer = AwsLambdaDeployer::new(
            AwsLambdaOptions {
                profile: Some("production".to_owned()),
                ..Default::default()
            },
            OperationContext::for_test(),
        );

        assert_eq!(
            deployer.aws_args(
                "lambda",
                "get-function",
                "us-west-2",
                &["--function-name", "api"]
            ),
            [
                "lambda",
                "get-function",
                "--function-name",
                "api",
                "--region",
                "us-west-2",
                "--profile",
                "production",
                "--no-cli-pager",
            ]
        );
    }

    #[test]
    fn requested_architecture_must_match_the_docker_artifact() {
        let deployer = AwsLambdaDeployer::new(
            AwsLambdaOptions {
                architecture: Some(LambdaArchitecture::Arm64),
                ..Default::default()
            },
            OperationContext::for_test(),
        );
        let artifact = RuntimeArtifact::Docker {
            directory: "docker".into(),
            image: "api".to_owned(),
            context: ".".into(),
            platform: Some("linux/amd64".to_owned()),
        };

        let error = deployer.architecture(&artifact).unwrap_err();

        assert_eq!(
            error.to_string(),
            "AWS Lambda architecture arm64 does not match the Docker artifact architecture x86_64"
        );
    }
}
