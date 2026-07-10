//! `shipit deploy` (port of cli.py's deploy command): deploy a Wasmer
//! package produced by `shipit build --wasmer`.


use anyhow::{bail, Result};

use crate::args::{DeployTargetArgs, ProjectArgs, WasmerConnArgs};
use crate::context::{resolve_environment, EnvironmentOptions};
use crate::paths::resolve_project_paths;

#[derive(clap::Args, Debug, Clone, Default)]
pub struct DeployArgs {
    #[command(flatten)]
    pub project: ProjectArgs,
    /// Deploy the project to Wasmer.
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
    if !args.project.path.exists() {
        bail!("The path {} does not exist", args.project.path.display());
    }
    let paths = resolve_project_paths(&args.project.path, args.project.subdir.as_deref())?;
    let environment = resolve_environment(
        &paths,
        &EnvironmentOptions {
            wasmer: true,
            wasmer_bin: args.conn.wasmer_bin.clone(),
            wasmer_registry: args.conn.wasmer_registry.clone(),
            wasmer_token: args.conn.wasmer_token.clone(),
            ..Default::default()
        },
    )?;
    let mut runner = environment.runner;
    let runner = runner
        .as_any()
        .downcast_mut::<shipit_run::wasmer::WasmerRunner>()
        .expect("resolve_environment(wasmer=true) yields a WasmerRunner");

    let wasmer_deploy = args.wasmer_deploy && !args.no_wasmer_deploy;
    if let Some(config_path) = &args.target.wasmer_deploy_config {
        runner.deploy_config(config_path)?;
    } else if wasmer_deploy {
        runner.deploy(
            args.target.wasmer_app_owner.as_deref(),
            args.target.wasmer_app_name.as_deref(),
        )?;
    }
    Ok(())
}
