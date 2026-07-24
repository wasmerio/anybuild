use anybuild::RunOptions;
use anyhow::Result;

use crate::args::{ProjectArgs, RunSelectionArgs};
use crate::commands::{client, execution};
use crate::SharedProjectArgs;

#[derive(clap::Args, Debug, Clone, Default)]
pub struct RunArgs {
    #[command(flatten)]
    pub project: ProjectArgs,
    #[arg(long)]
    pub wasmer: bool,
    #[arg(long)]
    pub wasmer_bin: Option<String>,
    #[arg(long)]
    pub docker: bool,
    #[arg(long)]
    pub docker_client: Option<String>,
    #[arg(long)]
    pub docker_opts: Option<String>,
    #[command(flatten)]
    pub selection: RunSelectionArgs,
    #[arg(long)]
    pub wasmer_registry: Option<String>,
    #[arg(long)]
    pub serve_port: Option<i64>,
}

pub fn run(args: RunArgs) -> Result<()> {
    let start = args.selection.effective_start();
    let after_deploy = args.selection.effective_after_deploy();
    let shared = SharedProjectArgs {
        path: args.project.path,
        subdir: args.project.subdir,
        ..Default::default()
    };
    let (build_environment, runtime_environment) = execution(
        args.wasmer,
        args.wasmer_bin,
        args.wasmer_registry,
        None,
        args.docker,
        args.docker_client,
        args.docker_opts,
    );
    let mut options = RunOptions {
        build_environment,
        runtime_environment,
        commands: args.selection.command_names,
        start,
        after_deploy,
        serve_port: args.serve_port,
        ..Default::default()
    };
    for spec in args.selection.volume_specs {
        let Some((name, path)) = spec.split_once(':') else {
            anyhow::bail!("Invalid volume mapping '{spec}'. Expected NAME:/guest/path");
        };
        options.volumes.push((name.to_owned(), path.to_owned()));
    }
    let outcome = client(&shared, args.serve_port)?.run(options)?;
    if outcome.executed.is_empty() && outcome.skipped.is_empty() {
        eprintln!("No commands specified. Use `--command` to run a command.");
    }
    Ok(())
}
