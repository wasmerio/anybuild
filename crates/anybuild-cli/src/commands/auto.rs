use anybuild::{
    AutoOptions, DeployOptions, DeployTarget, DeploymentPlatform, FlyOptions, GenerationPolicy,
    RunOptions, RuntimeEnvironment, WasmerOptions,
};
use anyhow::{bail, Result};

use crate::args::{
    DeployTargetArgs, DeploymentPlatformArg, ExecutionTargetArgs, FlyPlatformArgs,
    RunSelectionArgs, RunTarget,
};
use crate::commands::{build, client_with_render_options, execution, RenderOptions};
use crate::context::EnvironmentOptions;
use crate::SharedProjectArgs;

#[derive(clap::Args, Debug, Clone, Default)]
pub struct AutoArgs {
    #[command(flatten)]
    pub build: build::BuildArgs,
    #[command(flatten)]
    pub selection: RunSelectionArgs,
    /// Deploy to a platform after producing its runtime artifact.
    #[arg(long, value_enum)]
    pub platform: Option<DeploymentPlatformArg>,
    #[command(flatten)]
    pub fly: FlyPlatformArgs,
    /// Legacy shorthand for `--platform=wasmer`.
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
    let legacy_wasmer_deploy =
        args.wasmer_deploy || args.deploy_target.wasmer_deploy_config.is_some();
    if args.platform == Some(DeploymentPlatformArg::Fly) && legacy_wasmer_deploy {
        bail!("Wasmer deployment flags cannot be used with --platform=fly");
    }
    let platform = args.platform.or(if legacy_wasmer_deploy {
        Some(DeploymentPlatformArg::Wasmer)
    } else {
        None
    });
    let deploy_requested = platform.is_some();
    let skip_docker_if_safe = args.build.effective_skip_docker_if_safe_build();
    let start = args.selection.effective_start();
    let after_deploy = args.selection.effective_after_deploy();
    let mut execution_targets = args.build.targets.clone();
    apply_platform_runner(&mut execution_targets, platform);
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
        execution_targets,
        EnvironmentOptions {
            wasmer: args.build.wasmer,
            wasmer_bin: args.build.wasmer_conn.wasmer_bin.clone(),
            wasmer_registry: args.build.wasmer_conn.wasmer_registry.clone(),
            wasmer_token: args.build.wasmer_conn.wasmer_token.clone(),
            docker: args.build.docker,
            docker_client: args.build.docker_client.clone(),
            docker_opts: args.build.docker_opts.clone(),
        },
    )?;
    validate_platform_runtime(platform, &runtime_environment)?;
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
            platform: deployment_platform(
                platform.unwrap_or(DeploymentPlatformArg::Wasmer),
                &runtime_environment,
                &args.fly,
            ),
            target: DeployTarget::WriteConfig { path },
            process_io: Default::default(),
        })
    } else if deploy_requested {
        Some(DeployOptions {
            platform: deployment_platform(
                platform.unwrap_or(DeploymentPlatformArg::Wasmer),
                &runtime_environment,
                &args.fly,
            ),
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

fn validate_platform_runtime(
    platform: Option<DeploymentPlatformArg>,
    runtime: &RuntimeEnvironment,
) -> Result<()> {
    match (platform, runtime) {
        (Some(DeploymentPlatformArg::Wasmer), RuntimeEnvironment::Wasmer(_))
        | (Some(DeploymentPlatformArg::Fly), RuntimeEnvironment::Docker(_))
        | (None, _) => Ok(()),
        (Some(DeploymentPlatformArg::Wasmer), _) => {
            bail!("--platform=wasmer requires --runner=wasmer")
        }
        (Some(DeploymentPlatformArg::Fly), _) => {
            bail!("--platform=fly requires --runner=docker")
        }
    }
}

fn apply_platform_runner(
    targets: &mut ExecutionTargetArgs,
    platform: Option<DeploymentPlatformArg>,
) {
    if targets.runner.is_some() {
        return;
    }
    targets.runner = match platform {
        Some(DeploymentPlatformArg::Wasmer) => Some(RunTarget::Wasmer),
        Some(DeploymentPlatformArg::Fly) => Some(RunTarget::Docker),
        None => None,
    };
}

fn wasmer_options(runtime: &RuntimeEnvironment) -> WasmerOptions {
    match runtime {
        RuntimeEnvironment::Wasmer(options) => options.clone(),
        RuntimeEnvironment::Local | RuntimeEnvironment::Docker(_) => WasmerOptions::default(),
    }
}

fn deployment_platform(
    platform: DeploymentPlatformArg,
    runtime: &RuntimeEnvironment,
    fly: &FlyPlatformArgs,
) -> DeploymentPlatform {
    match platform {
        DeploymentPlatformArg::Wasmer => DeploymentPlatform::Wasmer(wasmer_options(runtime)),
        DeploymentPlatformArg::Fly => DeploymentPlatform::Fly(FlyOptions {
            binary: fly.fly_bin.clone(),
            token: fly.fly_token.clone(),
            app: fly.fly_app.clone(),
            config: fly.fly_config.clone(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fly_platform_selects_the_docker_runner() {
        let mut targets = ExecutionTargetArgs::default();

        apply_platform_runner(&mut targets, Some(DeploymentPlatformArg::Fly));

        assert_eq!(targets.runner, Some(RunTarget::Docker));
    }

    #[test]
    fn platform_does_not_override_an_explicit_runner() {
        let mut targets = ExecutionTargetArgs {
            runner: Some(RunTarget::Local),
            ..Default::default()
        };

        apply_platform_runner(&mut targets, Some(DeploymentPlatformArg::Fly));

        assert_eq!(targets.runner, Some(RunTarget::Local));
    }

    #[test]
    fn fly_platform_rejects_an_incompatible_runner() {
        let error =
            validate_platform_runtime(Some(DeploymentPlatformArg::Fly), &RuntimeEnvironment::Local)
                .unwrap_err();

        assert_eq!(error.to_string(), "--platform=fly requires --runner=docker");
    }
}
