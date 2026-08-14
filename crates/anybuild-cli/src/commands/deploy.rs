use anybuild::{
    AwsLambdaOptions, DeployOptions, DeployTarget, DeploymentPlatform, FlyOptions,
    LambdaArchitecture, WasmerOptions,
};
use anyhow::{bail, Result};

use crate::args::{
    AwsLambdaPlatformArgs, DeployTargetArgs, DeploymentPlatformArg, FlyPlatformArgs,
    LambdaArchitectureArg, ProjectArgs, WasmerConnArgs,
};
use crate::commands::client;
use crate::SharedProjectArgs;

#[derive(clap::Args, Debug, Clone, Default)]
pub struct DeployArgs {
    #[command(flatten)]
    pub project: ProjectArgs,
    /// Deployment platform receiving the runtime artifact.
    #[arg(long, value_enum, default_value = "wasmer")]
    pub platform: DeploymentPlatformArg,
    /// Legacy switch retained for Wasmer deployment compatibility.
    #[arg(long, default_value_t = true, overrides_with = "no_wasmer_deploy")]
    pub wasmer_deploy: bool,
    #[arg(long = "no-wasmer-deploy", hide = true)]
    pub no_wasmer_deploy: bool,
    #[command(flatten)]
    pub conn: WasmerConnArgs,
    #[command(flatten)]
    pub fly: FlyPlatformArgs,
    #[command(flatten)]
    pub aws_lambda: AwsLambdaPlatformArgs,
    #[command(flatten)]
    pub target: DeployTargetArgs,
}

pub fn run(args: DeployArgs) -> Result<()> {
    let shared = SharedProjectArgs {
        path: args.project.path,
        subdir: args.project.subdir,
        ..Default::default()
    };
    if args.platform != DeploymentPlatformArg::Wasmer && args.target.wasmer_deploy_config.is_some()
    {
        bail!("--wasmer-deploy-config can only be used with --platform=wasmer");
    }
    let target = if let Some(path) = args.target.wasmer_deploy_config {
        DeployTarget::WriteConfig { path }
    } else if args.platform != DeploymentPlatformArg::Wasmer
        || (args.wasmer_deploy && !args.no_wasmer_deploy)
    {
        DeployTarget::Publish {
            owner: args.target.wasmer_app_owner,
            name: args.target.wasmer_app_name,
        }
    } else {
        return Ok(());
    };
    client(&shared, None)?.deploy(DeployOptions {
        platform: match args.platform {
            DeploymentPlatformArg::Wasmer => DeploymentPlatform::Wasmer(WasmerOptions {
                binary: args.conn.wasmer_bin,
                registry: args.conn.wasmer_registry,
                token: args.conn.wasmer_token,
            }),
            DeploymentPlatformArg::Fly => DeploymentPlatform::Fly(FlyOptions {
                binary: args.fly.fly_bin,
                token: args.fly.fly_token,
                app: args.fly.fly_app,
                config: args.fly.fly_config,
            }),
            DeploymentPlatformArg::AwsLambda => DeploymentPlatform::AwsLambda(AwsLambdaOptions {
                binary: args.aws_lambda.aws_bin,
                docker_binary: args.aws_lambda.aws_docker_client,
                profile: args.aws_lambda.aws_profile,
                region: args.aws_lambda.aws_region,
                function: args.aws_lambda.aws_function,
                role: args.aws_lambda.aws_role,
                repository: args.aws_lambda.aws_repository,
                image_tag: args.aws_lambda.aws_image_tag,
                architecture: args.aws_lambda.aws_architecture.map(lambda_architecture),
            }),
        },
        target,
        process_io: Default::default(),
    })?;
    Ok(())
}

fn lambda_architecture(architecture: LambdaArchitectureArg) -> LambdaArchitecture {
    match architecture {
        LambdaArchitectureArg::X86_64 => LambdaArchitecture::X86_64,
        LambdaArchitectureArg::Arm64 => LambdaArchitecture::Arm64,
    }
}
