//! `shipit run` (port of cli.py's run command).
//!

use std::path::PathBuf;

use anyhow::Result;
use indexmap::IndexMap;
use shipit_build::local::ui::console_print;
use shipit_run::Runner;

use crate::context::{resolve_environment, EnvironmentOptions};
use crate::paths::resolve_project_paths;
use crate::volumes::{load_volume_mappings, merge_volume_mappings, parse_cli_volume_mappings};

/// `OPTIONAL_RUN_COMMANDS` in cli.py.
const OPTIONAL_RUN_COMMANDS: &[&str] = &["start", "after_deploy"];

#[derive(clap::Args, Debug, Clone, Default)]
pub struct RunArgs {
    /// Project path (defaults to current directory).
    #[arg(default_value = ".")]
    pub path: PathBuf,
    /// App subdirectory relative to the project path.
    #[arg(long)]
    pub subdir: Option<String>,
    /// Use Wasmer to run the project.
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
    /// Run one or more commands. Can be passed multiple times.
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
    /// Wasmer registry.
    #[arg(long)]
    pub wasmer_registry: Option<String>,
    /// The port to use (defaults to 8080).
    #[arg(long)]
    pub serve_port: Option<i64>,
}

/// Port of `resolve_run_commands`.
pub fn resolve_run_commands(
    command_names: &[String],
    start: bool,
    after_deploy: bool,
) -> Vec<String> {
    let mut commands: Vec<String> = command_names.to_vec();
    if after_deploy && !commands.iter().any(|c| c == "after_deploy") {
        commands.push("after_deploy".to_owned());
    }
    if start && !commands.iter().any(|c| c == "start") {
        commands.push("start".to_owned());
    }
    commands
}

/// Port of `runtime_serve_env`.
pub fn runtime_serve_env(serve_port: Option<i64>) -> IndexMap<String, String> {
    let port = match serve_port {
        Some(port) => port.to_string(),
        None => std::env::var("PORT").unwrap_or_else(|_| "8080".to_owned()),
    };
    IndexMap::from([("PORT".to_owned(), port)])
}

/// Port of `run_serve_commands`.
pub fn run_serve_commands(
    path: &std::path::Path,
    runner: &mut dyn Runner,
    commands: &[String],
    volume_specs: &[String],
    env: &IndexMap<String, String>,
    shipit_dir: Option<&std::path::Path>,
) -> Result<()> {
    let volume_mappings = merge_volume_mappings(&[
        load_volume_mappings(path, shipit_dir)?,
        parse_cli_volume_mappings(volume_specs)?,
    ]);
    for command in commands {
        if OPTIONAL_RUN_COMMANDS.contains(&command.as_str())
            && !runner.has_serve_command(command)
        {
            continue;
        }
        console_print(&format!("\nRunning command {command}"));
        runner.run_serve_command(command, Some(&volume_mappings), Some(env))?;
    }
    Ok(())
}

pub fn run(args: RunArgs) -> Result<()> {
    let paths = resolve_project_paths(&args.path, args.subdir.as_deref())?;
    let mut environment = resolve_environment(
        &paths,
        &EnvironmentOptions {
            wasmer: args.wasmer,
            wasmer_bin: args.wasmer_bin.clone(),
            wasmer_registry: args.wasmer_registry.clone(),
            wasmer_token: None,
            docker: args.docker,
            docker_client: args.docker_client.clone(),
            docker_opts: args.docker_opts.clone(),
        },
    )?;

    let commands_to_run =
        resolve_run_commands(&args.command_names, args.start, args.after_deploy);

    if !commands_to_run.is_empty() {
        run_serve_commands(
            &paths.workspace_root,
            environment.runner.as_mut(),
            &commands_to_run,
            &args.volume_specs,
            &runtime_serve_env(args.serve_port),
            Some(&environment.shipit_dir),
        )?;
    } else {
        console_print("No commands specified. Use `--command` to run a command.");
    }
    Ok(())
}
