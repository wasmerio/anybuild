//! `shipit auto` (port of cli.py's auto command): generate the Shipit file
//! if needed, build, optionally run commands, then optionally deploy.

use anyhow::{bail, Result};

use crate::args::{DeployTargetArgs, ProjectArgs, RunSelectionArgs};
use crate::commands::{build, deploy, generate, run};
use crate::paths::{default_shipit_path, resolve_project_paths};
use crate::SharedProjectArgs;

#[derive(clap::Args, Debug, Clone, Default)]
pub struct AutoArgs {
    /// Everything `shipit build` takes is a strict subset of auto's
    /// surface, so the whole struct is flattened in.
    #[command(flatten)]
    pub build: build::BuildArgs,
    #[command(flatten)]
    pub selection: RunSelectionArgs,
    /// Deploy the project to Wasmer.
    #[arg(long)]
    pub wasmer_deploy: bool,
    #[command(flatten)]
    pub deploy_target: DeployTargetArgs,
    /// Regenerate the Shipit file.
    #[arg(long, overrides_with = "no_regenerate")]
    pub regenerate: bool,
    #[arg(long = "no-regenerate", hide = true)]
    pub no_regenerate: bool,
    /// Use a temporary Shipit file in the system temporary directory.
    #[arg(long)]
    pub temp_shipit: bool,
}

pub fn run(args: AutoArgs) -> Result<()> {
    // We assume wasmer as an active flag if we pass wasmer deploy or wasmer
    // deploy config.
    let wasmer = args.build.wasmer
        || args.wasmer_deploy
        || args.deploy_target.wasmer_deploy_config.is_some();

    let project_paths = resolve_project_paths(
        &args.build.project.path,
        args.build.project.subdir.as_deref(),
    )?;

    let mut shipit_path = args.build.shipit_path.clone();
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
                install_command: args.build.install_command.clone(),
                build_command: args.build.build_command.clone(),
                start_command: args.build.start_command.clone(),
                provider: args.build.provider.clone(),
                config: args.build.config.clone(),
            },
            shipit_path.clone(),
        )?;
    }

    let mut build_args = args.build.clone();
    build_args.project.path = project_paths.workspace_root.clone();
    build_args.project.subdir = project_paths.subdir.clone();
    build_args.shipit_path = shipit_path;
    build_args.wasmer = wasmer;
    build::run(build_args)?;

    let start = args.selection.effective_start();
    let after_deploy = args.selection.effective_after_deploy();
    if !args.selection.command_names.is_empty()
        || !args.selection.volume_specs.is_empty()
        || start
        || after_deploy
    {
        run::run(run::RunArgs {
            project: ProjectArgs {
                path: project_paths.workspace_root.clone(),
                subdir: project_paths.subdir.clone(),
            },
            wasmer,
            wasmer_bin: args.build.wasmer_conn.wasmer_bin.clone(),
            docker: args.build.docker,
            docker_client: args.build.docker_client.clone(),
            docker_opts: args.build.docker_opts.clone(),
            selection: args.selection.clone(),
            wasmer_registry: args.build.wasmer_conn.wasmer_registry.clone(),
            serve_port: args.build.serve_port,
        })?;
    }

    if args.wasmer_deploy || args.deploy_target.wasmer_deploy_config.is_some() {
        deploy::run(deploy::DeployArgs {
            project: ProjectArgs {
                path: project_paths.workspace_root.clone(),
                subdir: project_paths.subdir.clone(),
            },
            wasmer_deploy: args.wasmer_deploy,
            no_wasmer_deploy: false,
            conn: args.build.wasmer_conn.clone(),
            target: args.deploy_target.clone(),
        })?;
    }

    Ok(())
}
