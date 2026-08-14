//! AWS Lambda managed-runtime and container-image deployment adapter.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use anyhow::{bail, Context, Result};
use indexmap::IndexMap;
use serde::Deserialize;

use crate::artifact::{ArtifactKind, RuntimeArtifact};
use crate::deploy::Deployer;
use crate::operation::OperationContext;
use crate::sdk::{AwsLambdaOptions, DeployOutcome, DeployTarget, LambdaArchitecture};

const LAMBDA_ADAPTER_IMAGE: &str = "public.ecr.aws/awsguru/aws-lambda-adapter:1.0.0";
const LAMBDA_ADAPTER_LAYER_VERSION: u32 = 28;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct FunctionLookup {
    configuration: FunctionConfiguration,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct FunctionConfiguration {
    package_type: String,
    #[serde(default)]
    environment: FunctionEnvironment,
    #[serde(default)]
    layers: Vec<FunctionLayer>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct FunctionEnvironment {
    #[serde(default)]
    variables: IndexMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct FunctionLayer {
    arn: String,
}

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

    fn function(&self, function: &str, region: &str) -> Result<Option<FunctionConfiguration>> {
        let args = self.aws_args(
            "lambda",
            "get-function",
            region,
            &["--function-name", function, "--output", "json"],
        );
        let output = self.capture(self.aws_binary(), &args, None)?;
        if output.status.success() {
            let lookup: FunctionLookup = serde_json::from_slice(&output.stdout)
                .context("AWS returned invalid Lambda function metadata")?;
            anyhow::ensure!(
                !lookup.configuration.package_type.is_empty(),
                "AWS returned Lambda function metadata without a package type"
            );
            return Ok(Some(lookup.configuration));
        }
        if String::from_utf8_lossy(&output.stderr).contains("ResourceNotFoundException") {
            return Ok(None);
        }
        Self::output_text(&output, "looking up the Lambda function")?;
        unreachable!()
    }

    fn publish_image_function(
        &self,
        function: &str,
        image_uri: &str,
        region: &str,
        architecture: LambdaArchitecture,
        existing: Option<&FunctionConfiguration>,
    ) -> Result<()> {
        let architecture = architecture.as_aws_value();
        let args = if existing.is_some() {
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

    fn publish_zip_function(
        &self,
        function: &str,
        artifact: &RuntimeArtifact,
        region: &str,
        architecture: LambdaArchitecture,
        existing: Option<&FunctionConfiguration>,
    ) -> Result<()> {
        let RuntimeArtifact::LambdaZip {
            archive,
            runtime,
            handler,
            environment: artifact_environment,
            ..
        } = artifact
        else {
            bail!("AWS managed runtime deployment requires a Lambda ZIP artifact")
        };
        anyhow::ensure!(
            archive.is_file(),
            "AWS Lambda archive is missing; rebuild with --builder=docker --runner=lambda ({})",
            archive.display()
        );
        let archive = std::path::absolute(archive)?;
        let archive = format!("fileb://{}", archive.display());
        let architecture = architecture.as_aws_value();
        let mut environment = existing
            .map(|function| function.environment.variables.clone())
            .unwrap_or_default();
        environment.extend(artifact_environment.clone());
        let environment = serde_json::to_string(&serde_json::json!({
            "Variables": environment,
        }))?;
        let layers = self.lambda_layers(existing, region, architecture)?;

        if existing.is_none() {
            let role = self
                .options
                .role
                .as_deref()
                .filter(|role| !role.is_empty())
                .context(
                    "Creating an AWS Lambda function requires --aws-role with an IAM role ARN",
                )?;
            let mut args = self.aws_args(
                "lambda",
                "create-function",
                region,
                &[
                    "--function-name",
                    function,
                    "--package-type",
                    "Zip",
                    "--runtime",
                    runtime,
                    "--handler",
                    handler,
                    "--zip-file",
                    &archive,
                    "--role",
                    role,
                    "--architectures",
                    architecture,
                    "--environment",
                    &environment,
                ],
            );
            add_layers(&mut args, &layers);
            return self.run(self.aws_binary(), &args);
        }

        let code = self.aws_args(
            "lambda",
            "update-function-code",
            region,
            &[
                "--function-name",
                function,
                "--zip-file",
                &archive,
                "--architectures",
                architecture,
            ],
        );
        self.run(self.aws_binary(), &code)?;
        self.wait_for_update(function, region)?;

        let mut configuration = self.aws_args(
            "lambda",
            "update-function-configuration",
            region,
            &[
                "--function-name",
                function,
                "--runtime",
                runtime,
                "--handler",
                handler,
                "--environment",
                &environment,
            ],
        );
        add_layers(&mut configuration, &layers);
        self.run(self.aws_binary(), &configuration)?;
        self.wait_for_update(function, region)
    }

    fn wait_for_update(&self, function: &str, region: &str) -> Result<()> {
        let args = self.aws_args(
            "lambda",
            "wait",
            region,
            &["function-updated-v2", "--function-name", function],
        );
        self.run(self.aws_binary(), &args)
    }

    fn lambda_layers(
        &self,
        existing: Option<&FunctionConfiguration>,
        region: &str,
        architecture: &str,
    ) -> Result<Vec<String>> {
        let layer = if let Some(layer) = self
            .options
            .adapter_layer
            .as_deref()
            .filter(|layer| !layer.is_empty())
        {
            layer.to_owned()
        } else {
            anyhow::ensure!(
                !region.starts_with("cn-") && !region.starts_with("us-gov-"),
                "Pass --aws-lambda-adapter-layer for AWS China or GovCloud regions"
            );
            let name = if architecture == "arm64" {
                "LambdaAdapterLayerArm64"
            } else {
                "LambdaAdapterLayerX86"
            };
            format!(
                "arn:aws:lambda:{region}:753240598075:layer:{name}:{LAMBDA_ADAPTER_LAYER_VERSION}"
            )
        };
        let mut layers: Vec<String> = existing
            .into_iter()
            .flat_map(|function| &function.layers)
            .map(|layer| layer.arn.clone())
            .filter(|arn| !arn.contains(":layer:LambdaAdapterLayer"))
            .collect();
        layers.push(layer);
        Ok(layers)
    }

    fn architecture(&self, artifact: &RuntimeArtifact) -> Result<LambdaArchitecture> {
        let artifact_architecture = match artifact.platform() {
            Some("linux/amd64") => LambdaArchitecture::X86_64,
            Some("linux/arm64") => LambdaArchitecture::Arm64,
            Some(platform) => bail!("Unsupported Docker platform for AWS Lambda: {platform}"),
            None if std::env::consts::ARCH == "aarch64" => LambdaArchitecture::Arm64,
            None => LambdaArchitecture::X86_64,
        };
        if let Some(requested) = self.options.architecture {
            anyhow::ensure!(
                requested == artifact_architecture,
                "AWS Lambda architecture {} does not match the runtime artifact architecture {}",
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

    fn artifact_kinds(&self) -> &'static [ArtifactKind] {
        &[ArtifactKind::LambdaZip, ArtifactKind::Docker]
    }

    fn load_legacy_artifact(&self, _anybuild_dir: &Path) -> Result<RuntimeArtifact> {
        bail!(
            "AWS Lambda deployment requires a Docker or Lambda ZIP artifact; build with --runner=lambda or --runner=docker first"
        )
    }

    fn deploy(
        &mut self,
        artifact: &RuntimeArtifact,
        target: DeployTarget,
    ) -> Result<DeployOutcome> {
        if matches!(target, DeployTarget::WriteConfig { .. }) {
            bail!("AWS Lambda does not support Wasmer deployment configs");
        }
        let function = self
            .options
            .function
            .as_deref()
            .filter(|function| !function.is_empty())
            .context("AWS Lambda deployment requires --aws-function")?;
        let region = self.resolve_region()?;
        let architecture = self.architecture(artifact)?;
        let existing = self.function(function, &region)?;
        let lambda_zip = artifact.lambda_zip();
        let use_zip = match existing
            .as_ref()
            .map(|function| function.package_type.as_str())
        {
            Some("Zip") => {
                anyhow::ensure!(
                    lambda_zip.is_some(),
                    "Existing AWS Lambda function uses Zip packages, but this service requires a Docker image"
                );
                true
            }
            Some("Image") => false,
            Some(package_type) => bail!("Unsupported AWS Lambda package type: {package_type}"),
            None => lambda_zip.is_some(),
        };

        if use_zip {
            self.publish_zip_function(
                function,
                lambda_zip.expect("checked above"),
                &region,
                architecture,
                existing.as_ref(),
            )?;
        } else {
            let (artifact_dir, local_image, _context) =
                artifact.docker_parts().with_context(|| {
                    "AWS Lambda image deployment requires a Docker artifact, but only a Lambda ZIP is available"
                })?;
            let dockerfile = std::fs::read_to_string(artifact_dir.join("Dockerfile"))
                .with_context(|| {
                    "Docker artifact metadata is missing; rebuild with --runner=docker"
                })?;
            anyhow::ensure!(
                dockerfile.contains(LAMBDA_ADAPTER_IMAGE),
                "Docker artifact predates AWS Lambda support; rebuild with --runner=docker"
            );
            let repository = self
                .options
                .repository
                .as_deref()
                .filter(|repository| !repository.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| repository_name(function));
            let repository_uri = self.ensure_repository(&repository, &region)?;
            let image_uri = self.push_image(local_image, &repository_uri, &region)?;
            self.publish_image_function(
                function,
                &image_uri,
                &region,
                architecture,
                existing.as_ref(),
            )?;
        }

        Ok(DeployOutcome::Published {
            owner: None,
            name: Some(function.to_owned()),
        })
    }
}

fn add_layers(args: &mut Vec<String>, layers: &[String]) {
    if !layers.is_empty() {
        let insert_at = args.len().saturating_sub(1);
        args.splice(
            insert_at..insert_at,
            std::iter::once("--layers".to_owned()).chain(layers.iter().cloned()),
        );
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
    fn requested_architecture_must_match_the_runtime_artifact() {
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
            "AWS Lambda architecture arm64 does not match the runtime artifact architecture x86_64"
        );
    }
}
