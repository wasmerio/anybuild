use anybuild::{DeployOptions, DeployTarget, WasmerOptions};
use anyhow::Result;

use crate::args::{DeployTargetArgs, ProjectArgs, WasmerConnArgs};
use crate::commands::client;
use crate::SharedProjectArgs;

#[derive(clap::Args, Debug, Clone, Default)]
pub struct DeployArgs {
    #[command(flatten)]
    pub project: ProjectArgs,
    #[arg(long, default_value_t = true, overrides_with = "no_wasmer_deploy")]
    pub wasmer_deploy: bool,
    #[arg(long = "no-wasmer-deploy", hide = true)]
    pub no_wasmer_deploy: bool,
    #[command(flatten)]
    pub conn: WasmerConnArgs,
    #[command(flatten)]
    pub target: DeployTargetArgs,
}

pub fn run(args: DeployArgs) -> Result<()> {
    let shared = SharedProjectArgs {
        path: args.project.path,
        subdir: args.project.subdir,
        ..Default::default()
    };
    let target = if let Some(path) = args.target.wasmer_deploy_config {
        DeployTarget::WriteConfig { path }
    } else if args.wasmer_deploy && !args.no_wasmer_deploy {
        DeployTarget::Publish {
            owner: args.target.wasmer_app_owner,
            name: args.target.wasmer_app_name,
        }
    } else {
        return Ok(());
    };
    client(&shared, None)?.deploy(DeployOptions {
        wasmer: WasmerOptions {
            binary: args.conn.wasmer_bin,
            registry: args.conn.wasmer_registry,
            token: args.conn.wasmer_token,
        },
        target,
        process_io: Default::default(),
    })?;
    Ok(())
}
