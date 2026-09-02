//! anybuild — detect, build, and serve projects.
//!
//! The command surface covers generation, planning, building, running, and deployment.

mod args;
mod commands;
mod context;
mod render;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "anybuild",
    version,
    disable_version_flag = true,
    about = "Ship any project"
)]
struct Cli {
    /// Show the version and exit.
    #[arg(short = 'v', long = "version")]
    _version: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Args, Debug, Default, Clone)]
struct SharedProjectArgs {
    /// Project path (defaults to current directory).
    #[arg(default_value = ".")]
    path: PathBuf,
    /// App subdirectory relative to the project path.
    #[arg(long)]
    subdir: Option<String>,
    /// The install command to use (overwrites the default)
    #[arg(long)]
    install_command: Option<String>,
    /// The build command to use (overwrites the default)
    #[arg(long)]
    build_command: Option<String>,
    /// The start command to use (overwrites the default)
    #[arg(long)]
    start_command: Option<String>,
    /// Use a specific provider to build the project.
    #[arg(long)]
    provider: Option<String>,
    /// The JSON content to use as input.
    #[arg(long)]
    config: Option<String>,
    /// Set a build variable, repeatable (`--env NAME` or `--env NAME=VALUE`).
    #[arg(long = "env", value_name = "NAME[=VALUE]", value_parser = args::parse_env_arg)]
    env: Vec<(String, Option<String>)>,
}

#[derive(Subcommand)]
enum Command {
    /// Generate the Anybuild file if needed, build, and optionally run.
    Auto(Box<commands::auto::AutoArgs>),
    /// Create or refresh the Anybuild file.
    Generate {
        #[command(flatten)]
        shared: SharedProjectArgs,
        /// Output path of the generated Anybuild file.
        #[arg(short, long, alias = "output")]
        out: Option<PathBuf>,
        /// Check whether the persisted provider config matches detection.
        #[arg(long)]
        check: bool,
    },
    /// Evaluate the project and emit config, commands, and services.
    Plan {
        #[command(flatten)]
        shared: SharedProjectArgs,
        /// Output path of the plan (defaults to stdout).
        #[arg(short, long, alias = "output")]
        out: Option<PathBuf>,
        /// The path to the Anybuild file.
        #[arg(long, alias = "shipit-path")]
        anybuild_path: Option<PathBuf>,
        /// Regenerate the Anybuild file.
        #[arg(long)]
        regenerate: bool,
        /// Use a temporary Anybuild file in the system temporary directory.
        #[arg(long, alias = "temp-shipit")]
        temp_anybuild: bool,
        /// The port to use (defaults to 8080).
        #[arg(long)]
        serve_port: Option<i64>,
        /// Evaluate the plan the way the Wasmer runner would.
        #[arg(long)]
        wasmer: bool,
        /// The path to the Wasmer binary.
        #[arg(long)]
        wasmer_bin: Option<String>,
        /// Wasmer registry.
        #[arg(long)]
        wasmer_registry: Option<String>,
        /// Wasmer token.
        #[arg(long)]
        wasmer_token: Option<String>,
        /// Evaluate the plan the way the Docker backend would.
        #[arg(long)]
        docker: bool,
        /// Use a specific Docker client (such as depot, podman, etc.)
        #[arg(long)]
        docker_client: Option<String>,
    },
    /// Run the build steps defined in Anybuild.
    Build(commands::build::BuildArgs),
    /// Run explicit commands for the project.
    Run(commands::run::RunArgs),
    /// Deploy a runtime artifact to a deployment platform.
    Deploy(commands::deploy::DeployArgs),
}

/// The typer callback's `print_help()`: a rounded panel with the version,
/// printed to stderr before every command except `plan`.
fn print_banner() {
    let content = format!("Anybuild {}", anybuild::version());
    let width = content.chars().count() + 2;
    eprintln!("╭{}╮", "─".repeat(width));
    eprintln!("│ {content} │");
    eprintln!("╰{}╯", "─".repeat(width));
    eprintln!();
}

fn main() {
    // Port of cli.py's main(): if no subcommand, or the first token looks
    // like an option or a path, default to "auto".
    const KNOWN_COMMANDS: &[&str] = &["auto", "generate", "plan", "build", "run", "deploy"];
    let mut args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if matches!(
        args.get(1).and_then(|arg| arg.to_str()),
        Some("--version" | "-v")
    ) {
        println!("{}", anybuild::version());
        return;
    }
    let needs_auto = match args.get(1) {
        None => true,
        Some(first) => {
            let first = first.to_string_lossy();
            first.starts_with('-') || !KNOWN_COMMANDS.contains(&first.as_ref())
        }
    };
    if needs_auto {
        args.insert(1, "auto".into());
    }

    let cli = Cli::parse_from(args);
    if !matches!(cli.command, Command::Plan { .. }) {
        print_banner();
    }
    let result = match cli.command {
        Command::Auto(args) => commands::auto::run(*args),
        Command::Generate { shared, out, check } => commands::generate::run(shared, out, check),
        Command::Plan {
            shared,
            out,
            anybuild_path,
            regenerate,
            temp_anybuild,
            serve_port,
            wasmer,
            wasmer_bin,
            wasmer_registry,
            wasmer_token,
            docker,
            docker_client,
        } => commands::plan::run(
            shared,
            out,
            anybuild_path,
            regenerate,
            temp_anybuild,
            serve_port,
            context::EnvironmentOptions {
                wasmer,
                wasmer_bin,
                wasmer_registry,
                wasmer_token,
                docker,
                docker_client,
                docker_opts: None,
            },
        ),
        Command::Build(args) => commands::build::run(args),
        Command::Run(args) => commands::run::run(args),
        Command::Deploy(args) => commands::deploy::run(args),
    };
    if let Err(err) = result {
        eprintln!("Error: {err:#}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use crate::args::{BuildTarget, DeploymentPlatformArg, RunTarget};

    /// clap's built-in definition validation: catches flatten collisions,
    /// duplicate flags, and bad override references at test time.
    #[test]
    fn cli_definition_is_valid() {
        use clap::CommandFactory;
        super::Cli::command().debug_assert();
    }

    // Port of test_cli_after_deploy.py's test_auto_passes_* quartet. The
    // Python tests monkeypatch cli.run and inspect the kwargs auto passes
    // along; in Rust auto.rs forwards `selection` / `serve_port` to
    // run::run by direct struct construction, so the observable seam is
    // the parsed AutoArgs.
    fn parse_auto(args: &[&str]) -> crate::commands::auto::AutoArgs {
        use clap::Parser;
        let mut argv = vec!["anybuild"];
        argv.extend_from_slice(args);
        match super::Cli::try_parse_from(argv).expect("parses").command {
            super::Command::Auto(args) => *args,
            _ => panic!("expected the auto command"),
        }
    }

    fn parse_deploy(args: &[&str]) -> crate::commands::deploy::DeployArgs {
        use clap::Parser;
        let mut argv = vec!["anybuild"];
        argv.extend_from_slice(args);
        match super::Cli::try_parse_from(argv).expect("parses").command {
            super::Command::Deploy(args) => args,
            _ => panic!("expected the deploy command"),
        }
    }

    #[test]
    fn auto_passes_after_deploy_to_run() {
        let args = parse_auto(&["auto", "proj", "--start", "--after-deploy"]);
        assert!(args.selection.effective_after_deploy());
        assert!(args.selection.effective_start());
    }

    #[test]
    fn auto_passes_serve_port_to_run() {
        let args = parse_auto(&["auto", "proj", "--start", "--serve-port=34567"]);
        assert!(args.selection.effective_start());
        assert_eq!(args.build.serve_port, Some(34567));
    }

    #[test]
    fn auto_passes_commands_to_run() {
        let args = parse_auto(&["auto", "proj", "--command=prepare-db", "-c", "warm-cache"]);
        assert_eq!(args.selection.command_names, ["prepare-db", "warm-cache"]);
    }

    #[test]
    fn auto_passes_volume_specs_to_run() {
        let args = parse_auto(&[
            "auto",
            "proj",
            "--volume",
            "uploads:/app/uploads",
            "--volume",
            "cache:/app/cache",
        ]);
        assert_eq!(
            args.selection.volume_specs,
            ["uploads:/app/uploads", "cache:/app/cache"]
        );
    }

    #[test]
    fn auto_accepts_detailed_build_steps() {
        let args = parse_auto(&["auto", "proj", "--show-detailed-steps"]);
        assert!(args.build.show_detailed_steps);
    }

    #[test]
    fn auto_accepts_wasmer_file_output() {
        let args = parse_auto(&["auto", "proj", "--show-wasmer-files"]);
        assert!(args.build.show_wasmer_files);
    }

    #[test]
    fn auto_accepts_explicit_build_and_run_targets() {
        let args = parse_auto(&["auto", "proj", "--builder=docker", "--runner=wasmer"]);
        assert_eq!(args.build.targets.builder, Some(BuildTarget::Docker));
        assert_eq!(args.build.targets.runner, Some(RunTarget::Wasmer));
    }

    #[test]
    fn auto_accepts_explicit_local_targets() {
        let args = parse_auto(&["auto", "proj", "--builder=local", "--runner=local"]);
        assert_eq!(args.build.targets.builder, Some(BuildTarget::Local));
        assert_eq!(args.build.targets.runner, Some(RunTarget::Local));
    }

    #[test]
    fn auto_accepts_docker_runner() {
        let args = parse_auto(&["auto", "proj", "--runner=docker"]);
        assert_eq!(args.build.targets.runner, Some(RunTarget::Docker));
    }

    #[test]
    fn auto_accepts_lambda_runner() {
        let args = parse_auto(&["auto", "proj", "--runner=lambda"]);
        assert_eq!(args.build.targets.runner, Some(RunTarget::Lambda));
    }

    #[test]
    fn auto_accepts_wasmer_deploy_without_app_identity() {
        let args = parse_auto(&["auto", "proj", "--wasmer-deploy"]);
        assert!(args.wasmer_deploy);
        assert_eq!(args.deploy_target.wasmer_app_owner, None);
        assert_eq!(args.deploy_target.wasmer_app_name, None);
    }

    #[test]
    fn auto_accepts_explicit_deployment_platform() {
        let args = parse_auto(&["auto", "proj", "--platform=wasmer"]);
        assert_eq!(args.platform, Some(DeploymentPlatformArg::Wasmer));
        assert!(!args.wasmer_deploy);
    }

    #[test]
    fn auto_accepts_fly_deployment_options() {
        let args = parse_auto(&[
            "auto",
            "proj",
            "--platform=fly",
            "--fly-app=example-api",
            "--fly-token=secret",
        ]);
        assert_eq!(args.platform, Some(DeploymentPlatformArg::Fly));
        assert_eq!(args.fly.fly_app.as_deref(), Some("example-api"));
        assert_eq!(args.fly.fly_token.as_deref(), Some("secret"));
    }

    #[test]
    fn auto_accepts_aws_lambda_deployment_options() {
        let args = parse_auto(&[
            "auto",
            "proj",
            "--platform=aws-lambda",
            "--aws-function=example-api",
            "--aws-region=us-west-2",
            "--aws-architecture=arm64",
            "--aws-lambda-adapter-layer=arn:aws:lambda:us-west-2:123:layer:adapter:1",
        ]);
        assert_eq!(args.platform, Some(DeploymentPlatformArg::AwsLambda));
        assert_eq!(args.aws_lambda.aws_function.as_deref(), Some("example-api"));
        assert_eq!(args.aws_lambda.aws_region.as_deref(), Some("us-west-2"));
        assert_eq!(
            args.aws_lambda.aws_architecture,
            Some(crate::args::LambdaArchitectureArg::Arm64)
        );
        assert_eq!(
            args.aws_lambda.aws_lambda_adapter_layer.as_deref(),
            Some("arn:aws:lambda:us-west-2:123:layer:adapter:1")
        );
    }

    #[test]
    fn deploy_accepts_wasmer_deploy_without_app_identity() {
        let args = parse_deploy(&["deploy", "proj", "--wasmer-deploy"]);
        assert!(args.wasmer_deploy);
        assert_eq!(args.target.wasmer_app_owner, None);
        assert_eq!(args.target.wasmer_app_name, None);
    }

    #[test]
    fn deploy_defaults_to_wasmer_platform() {
        let args = parse_deploy(&["deploy", "proj"]);
        assert_eq!(args.platform, DeploymentPlatformArg::Wasmer);
    }

    #[test]
    fn deploy_accepts_fly_deployment_options() {
        let args = parse_deploy(&[
            "deploy",
            "proj",
            "--platform=fly",
            "--fly-app=example-api",
            "--fly-config=deploy/fly.toml",
        ]);
        assert_eq!(args.platform, DeploymentPlatformArg::Fly);
        assert_eq!(args.fly.fly_app.as_deref(), Some("example-api"));
        assert_eq!(
            args.fly.fly_config.as_deref(),
            Some(std::path::Path::new("deploy/fly.toml"))
        );
    }

    #[test]
    fn deploy_accepts_aws_lambda_alias_and_options() {
        let args = parse_deploy(&[
            "deploy",
            "proj",
            "--platform=lambda",
            "--aws-function=example-api",
            "--aws-role=arn:aws:iam::123456789012:role/lambda",
        ]);
        assert_eq!(args.platform, DeploymentPlatformArg::AwsLambda);
        assert_eq!(args.aws_lambda.aws_function.as_deref(), Some("example-api"));
        assert!(args
            .aws_lambda
            .aws_role
            .as_deref()
            .unwrap()
            .ends_with("role/lambda"));
    }
}
