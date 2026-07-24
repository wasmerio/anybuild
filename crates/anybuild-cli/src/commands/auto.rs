//! `anybuild auto` (port of cli.py's auto command): generate the Anybuild file
//! if needed, build, optionally run commands, then optionally deploy.

use anyhow::{bail, Result};

use crate::args::{DeployTargetArgs, ProjectArgs, RunSelectionArgs};
use crate::commands::{build, deploy, generate, run};
use crate::paths::{migrate_legacy_anybuild, resolve_project_paths};
use crate::SharedProjectArgs;

#[derive(clap::Args, Debug, Clone, Default)]
pub struct AutoArgs {
    /// Everything `anybuild build` takes is a strict subset of auto's
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
    /// Regenerate the Anybuild file.
    #[arg(long, overrides_with = "no_regenerate")]
    pub regenerate: bool,
    #[arg(long = "no-regenerate", hide = true)]
    pub no_regenerate: bool,
    /// Use a temporary Anybuild file in the system temporary directory.
    #[arg(long, alias = "temp-shipit")]
    pub temp_anybuild: bool,
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

    let mut anybuild_path = args.build.anybuild_path.clone();
    if args.temp_anybuild {
        if anybuild_path.is_some() {
            bail!("Cannot use both --temp-anybuild and --anybuild-path");
        }
        // tempfile.NamedTemporaryFile(delete=False, prefix="Anybuild")
        let file = tempfile::Builder::new().prefix("Anybuild").tempfile()?;
        let (_file, path) = file.keep()?;
        anybuild_path = Some(path);
    }

    let mut regenerate = args.regenerate && !args.no_regenerate;
    if !regenerate {
        match &anybuild_path {
            Some(path) if !path.exists() => regenerate = true,
            None if migrate_legacy_anybuild(&project_paths)?.is_none() => regenerate = true,
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
            anybuild_path.clone(),
        )?;
    }

    let mut build_args = args.build.clone();
    build_args.project.path = project_paths.workspace_root.clone();
    build_args.project.subdir = project_paths.subdir.clone();
    build_args.anybuild_path = anybuild_path;
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
