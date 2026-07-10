//! `shipit auto` (port of cli.py's auto command): generate the Shipit file
//! if needed, build, optionally run commands, then optionally deploy.

use std::path::PathBuf;

use anyhow::{bail, Result};

use crate::commands::{build, deploy, generate, run};
use crate::paths::{default_shipit_path, resolve_project_paths};
use crate::SharedProjectArgs;

#[derive(clap::Args, Debug, Clone, Default)]
pub struct AutoArgs {
    /// Project path (defaults to current directory).
    #[arg(default_value = ".")]
    pub path: PathBuf,
    /// App subdirectory relative to the project path.
    #[arg(long)]
    pub subdir: Option<String>,
    /// Use Wasmer to build and run the project.
    #[arg(long)]
    pub wasmer: bool,
    /// The path to the Wasmer binary.
    #[arg(long)]
    pub wasmer_bin: Option<String>,
    /// Use Docker to build the project.
    #[arg(long)]
    pub docker: bool,
    /// Use a specific Docker client (such as depot, podman, etc.)
    #[arg(long)]
    pub docker_client: Option<String>,
    /// Additional options to pass to the Docker client.
    #[arg(long)]
    pub docker_opts: Option<String>,
    /// Skip Docker if the build can be done safely locally (only copy commands).
    #[arg(long, overrides_with = "no_skip_docker_if_safe_build")]
    pub skip_docker_if_safe_build: bool,
    #[arg(long = "no-skip-docker-if-safe-build", hide = true)]
    pub no_skip_docker_if_safe_build: bool,
    /// Run the prepare command after building (defaults to True).
    #[arg(long)]
    pub skip_prepare: bool,
    /// Run one or more commands after building. Can be passed multiple times.
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
    /// Regenerate the Shipit file.
    #[arg(long, overrides_with = "no_regenerate")]
    pub regenerate: bool,
    #[arg(long = "no-regenerate", hide = true)]
    pub no_regenerate: bool,
    /// The path to the Shipit file (defaults to Shipit or Shipit.<subdir>).
    #[arg(long)]
    pub shipit_path: Option<PathBuf>,
    /// Use a temporary Shipit file in the system temporary directory.
    #[arg(long)]
    pub temp_shipit: bool,
    /// Deploy the project to Wasmer.
    #[arg(long)]
    pub wasmer_deploy: bool,
    /// Save the output of the Wasmer build to a json file
    #[arg(long)]
    pub wasmer_deploy_config: Option<PathBuf>,
    /// Wasmer token.
    #[arg(long)]
    pub wasmer_token: Option<String>,
    /// Wasmer registry.
    #[arg(long)]
    pub wasmer_registry: Option<String>,
    /// Owner of the Wasmer app.
    #[arg(long)]
    pub wasmer_app_owner: Option<String>,
    /// Name of the Wasmer app.
    #[arg(long)]
    pub wasmer_app_name: Option<String>,
    /// The install command to use (overwrites the default)
    #[arg(long)]
    pub install_command: Option<String>,
    /// The build command to use (overwrites the default)
    #[arg(long)]
    pub build_command: Option<String>,
    /// The start command to use (overwrites the default)
    #[arg(long)]
    pub start_command: Option<String>,
    /// The environment to use (defaults to `.env`, it will use .env.<env_name> if provided)
    #[arg(long)]
    pub env_name: Option<String>,
    /// Use a specific provider to build the project.
    #[arg(long)]
    pub provider: Option<String>,
    /// The JSON content to use as input.
    #[arg(long)]
    pub config: Option<String>,
    /// The port to use (defaults to 8080).
    #[arg(long)]
    pub serve_port: Option<i64>,
}

pub fn run(args: AutoArgs) -> Result<()> {
    // We assume wasmer as an active flag if we pass wasmer deploy or wasmer
    // deploy config.
    let wasmer = args.wasmer || args.wasmer_deploy || args.wasmer_deploy_config.is_some();

    let project_paths = resolve_project_paths(&args.path, args.subdir.as_deref())?;

    let mut shipit_path = args.shipit_path.clone();
    if args.temp_shipit {
        if shipit_path.is_some() {
            bail!("Cannot use both --temp-shipit and --shipit-path");
        }
        // tempfile.NamedTemporaryFile(delete=False, prefix="Shipit")
        let file = tempfile::Builder::new().prefix("Shipit").tempfile()?;
        let (_file, path) = file.keep()?;
        shipit_path = Some(path);
    }

    let mut regenerate = args.regenerate && !args.no_regenerate;
    if !regenerate {
        match &shipit_path {
            Some(path) if !path.exists() => regenerate = true,
            None if !default_shipit_path(&project_paths).exists() => regenerate = true,
            _ => {}
        }
    }

    if regenerate {
        generate::run(
            SharedProjectArgs {
                path: project_paths.workspace_root.clone(),
                subdir: project_paths.subdir.clone(),
                install_command: args.install_command.clone(),
                build_command: args.build_command.clone(),
                start_command: args.start_command.clone(),
                provider: args.provider.clone(),
                config: args.config.clone(),
            },
            shipit_path.clone(),
        )?;
    }

    build::run(build::BuildArgs {
        path: project_paths.workspace_root.clone(),
        subdir: project_paths.subdir.clone(),
        shipit_path,
        start_command: args.start_command.clone(),
        install_command: args.install_command.clone(),
        build_command: args.build_command.clone(),
        wasmer,
        skip_prepare: args.skip_prepare,
        wasmer_bin: args.wasmer_bin.clone(),
        wasmer_registry: args.wasmer_registry.clone(),
        wasmer_token: args.wasmer_token.clone(),
        docker: args.docker,
        docker_client: args.docker_client.clone(),
        docker_opts: args.docker_opts.clone(),
        skip_docker_if_safe_build: args.skip_docker_if_safe_build,
        no_skip_docker_if_safe_build: args.no_skip_docker_if_safe_build,
        env_name: args.env_name.clone(),
        serve_port: args.serve_port,
        provider: args.provider.clone(),
        config: args.config.clone(),
    })?;

    let start = args.start && !args.no_start;
    let after_deploy = args.after_deploy && !args.no_after_deploy;
    if !args.command_names.is_empty() || !args.volume_specs.is_empty() || start || after_deploy
    {
        run::run(run::RunArgs {
            path: project_paths.workspace_root.clone(),
            subdir: project_paths.subdir.clone(),
            wasmer,
            wasmer_bin: args.wasmer_bin.clone(),
            docker: args.docker,
            docker_client: args.docker_client.clone(),
            docker_opts: args.docker_opts.clone(),
            command_names: args.command_names.clone(),
            volume_specs: args.volume_specs.clone(),
            start,
            no_start: false,
            after_deploy,
            no_after_deploy: false,
            wasmer_registry: args.wasmer_registry.clone(),
            serve_port: args.serve_port,
        })?;
    }

    if args.wasmer_deploy || args.wasmer_deploy_config.is_some() {
        deploy::run(deploy::DeployArgs {
            path: project_paths.workspace_root.clone(),
            subdir: project_paths.subdir.clone(),
            wasmer_deploy: args.wasmer_deploy,
            no_wasmer_deploy: false,
            wasmer_bin: args.wasmer_bin.clone(),
            wasmer_token: args.wasmer_token.clone(),
            wasmer_registry: args.wasmer_registry.clone(),
            wasmer_app_owner: args.wasmer_app_owner.clone(),
            wasmer_app_name: args.wasmer_app_name.clone(),
            wasmer_deploy_config: args.wasmer_deploy_config.clone(),
        })?;
    }

    Ok(())
}
