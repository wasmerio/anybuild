use anybuild::{
    AutoOptions, DeployOptions, DeployTarget, GenerationPolicy, RunOptions, RuntimeEnvironment,
    WasmerOptions,
};
use anyhow::{bail, Result};

use crate::args::{DeployTargetArgs, RunSelectionArgs};
use crate::commands::{build, client_with_render_options, execution, RenderOptions};
use crate::context::EnvironmentOptions;
use crate::SharedProjectArgs;

#[derive(clap::Args, Debug, Clone, Default)]
pub struct AutoArgs {
    #[command(flatten)]
    pub build: build::BuildArgs,
    #[command(flatten)]
    pub selection: RunSelectionArgs,
    #[arg(long)]
    pub wasmer_deploy: bool,
    #[command(flatten)]
    pub deploy_target: DeployTargetArgs,
    #[arg(long, overrides_with = "no_regenerate")]
    pub regenerate: bool,
    #[arg(long = "no-regenerate", hide = true)]
    pub no_regenerate: bool,
    #[arg(long, alias = "temp-shipit")]
    pub temp_anybuild: bool,
}

pub fn run(args: AutoArgs) -> Result<()> {
    if args.temp_anybuild && args.build.anybuild_path.is_some() {
        bail!("Cannot use both --temp-anybuild and --anybuild-path");
    }
    let deploy_requested = args.wasmer_deploy || args.deploy_target.wasmer_deploy_config.is_some();
    let skip_docker_if_safe = args.build.effective_skip_docker_if_safe_build();
    let start = args.selection.effective_start();
    let after_deploy = args.selection.effective_after_deploy();
    let wasmer = args.build.wasmer || deploy_requested;
    let shared = SharedProjectArgs {
        path: args.build.project.path.clone(),
        subdir: args.build.project.subdir.clone(),
        install_command: args.build.install_command.clone(),
        build_command: args.build.build_command.clone(),
        start_command: args.build.start_command.clone(),
        provider: args.build.provider.clone(),
        config: args.build.config.clone(),
    };
    let (build_environment, runtime_environment) = execution(
        args.build.targets.clone(),
        EnvironmentOptions {
            wasmer,
            wasmer_bin: args.build.wasmer_conn.wasmer_bin.clone(),
            wasmer_registry: args.build.wasmer_conn.wasmer_registry.clone(),
            wasmer_token: args.build.wasmer_conn.wasmer_token.clone(),
            docker: args.build.docker,
            docker_client: args.build.docker_client.clone(),
            docker_opts: args.build.docker_opts.clone(),
        },
    )?;
    let build_options = anybuild::BuildOptions {
        anybuild_path: args.build.anybuild_path,
        build_environment: build_environment.clone(),
        runtime_environment: runtime_environment.clone(),
        skip_prepare: args.build.skip_prepare,
        skip_docker_if_safe,
        env_name: args.build.env_name,
        ..Default::default()
    };
    let run_requested = !args.selection.command_names.is_empty()
        || !args.selection.volume_specs.is_empty()
        || start
        || after_deploy;
    let run = if run_requested {
        let mut options = RunOptions {
            build_environment,
            runtime_environment: runtime_environment.clone(),
            commands: args.selection.command_names,
            start,
            after_deploy,
            serve_port: args.build.serve_port,
            ..Default::default()
        };
        for spec in args.selection.volume_specs {
            let Some((name, path)) = spec.split_once(':') else {
                bail!("Invalid volume mapping '{spec}'. Expected NAME:/guest/path");
            };
            options.volumes.push((name.to_owned(), path.to_owned()));
        }
        Some(options)
    } else {
        None
    };
    let deploy = if let Some(path) = args.deploy_target.wasmer_deploy_config {
        Some(DeployOptions {
            wasmer: wasmer_options(&runtime_environment),
            target: DeployTarget::WriteConfig { path },
            process_io: Default::default(),
        })
    } else if args.wasmer_deploy {
        Some(DeployOptions {
            wasmer: wasmer_options(&runtime_environment),
            target: DeployTarget::Publish {
                owner: args.deploy_target.wasmer_app_owner,
                name: args.deploy_target.wasmer_app_name,
            },
            process_io: Default::default(),
        })
    } else {
        None
    };
    let generation = if args.temp_anybuild {
        GenerationPolicy::Temporary
    } else if args.regenerate && !args.no_regenerate {
        GenerationPolicy::Always
    } else {
        GenerationPolicy::IfMissing
    };
    client_with_render_options(
        &shared,
        args.build.serve_port,
        RenderOptions {
            show_detailed_steps: args.build.show_detailed_steps,
            show_wasmer_files: args.build.show_wasmer_files,
        },
    )?
    .auto(AutoOptions {
        generation,
        build: build_options,
        run,
        deploy,
    })?;
    Ok(())
}

fn wasmer_options(runtime: &RuntimeEnvironment) -> WasmerOptions {
    match runtime {
        RuntimeEnvironment::Wasmer(options) => options.clone(),
        RuntimeEnvironment::Local | RuntimeEnvironment::Docker(_) => WasmerOptions::default(),
    }
}
