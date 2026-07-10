//! `shipit deploy` (port of cli.py's deploy command): deploy a Wasmer
//! package produced by `shipit build --wasmer`.

use std::path::PathBuf;

use anyhow::{bail, Result};

use crate::context::{resolve_environment, EnvironmentOptions};
use crate::paths::resolve_project_paths;

#[derive(clap::Args, Debug, Clone, Default)]
pub struct DeployArgs {
    /// Project path (defaults to current directory).
    #[arg(default_value = ".")]
    pub path: PathBuf,
    /// App subdirectory relative to the project path.
    #[arg(long)]
    pub subdir: Option<String>,
    /// Deploy the project to Wasmer.
    #[arg(long, default_value_t = true, overrides_with = "no_wasmer_deploy")]
    pub wasmer_deploy: bool,
    #[arg(long = "no-wasmer-deploy", hide = true)]
    pub no_wasmer_deploy: bool,
    /// The path to the Wasmer binary.
    #[arg(long)]
    pub wasmer_bin: Option<String>,
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
    /// Save the output of the Wasmer build to a json file
    #[arg(long)]
    pub wasmer_deploy_config: Option<PathBuf>,
}

pub fn run(args: DeployArgs) -> Result<()> {
    if !args.path.exists() {
        bail!("The path {} does not exist", args.path.display());
    }
    let paths = resolve_project_paths(&args.path, args.subdir.as_deref())?;
    let environment = resolve_environment(
        &paths,
        &EnvironmentOptions {
            wasmer: true,
            wasmer_bin: args.wasmer_bin.clone(),
            wasmer_registry: args.wasmer_registry.clone(),
            wasmer_token: args.wasmer_token.clone(),
            ..Default::default()
        },
    )?;
    let mut runner = environment.runner;
    let runner = runner
        .as_any()
        .downcast_mut::<shipit_run::wasmer::WasmerRunner>()
        .expect("resolve_environment(wasmer=true) yields a WasmerRunner");

    let wasmer_deploy = args.wasmer_deploy && !args.no_wasmer_deploy;
    if let Some(config_path) = &args.wasmer_deploy_config {
        runner.deploy_config(config_path)?;
    } else if wasmer_deploy {
        runner.deploy(
            args.wasmer_app_owner.as_deref(),
            args.wasmer_app_name.as_deref(),
        )?;
    }
    Ok(())
}
