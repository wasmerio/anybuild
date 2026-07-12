//! `shipit run` (port of cli.py's run command).
//!


use anyhow::Result;
use indexmap::IndexMap;
use shipit_build::local::ui::console_print;
use shipit_run::Runner;

use crate::args::{ProjectArgs, RunSelectionArgs};
use crate::context::{resolve_environment, EnvironmentOptions};
use crate::paths::resolve_project_paths;
use crate::volumes::{load_volume_mappings, merge_volume_mappings, parse_cli_volume_mappings};

/// `OPTIONAL_RUN_COMMANDS` in cli.py.
const OPTIONAL_RUN_COMMANDS: &[&str] = &["start", "after_deploy"];

#[derive(clap::Args, Debug, Clone, Default)]
pub struct RunArgs {
    #[command(flatten)]
    pub project: ProjectArgs,
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
    #[command(flatten)]
    pub selection: RunSelectionArgs,
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
    let paths = resolve_project_paths(&args.project.path, args.project.subdir.as_deref())?;
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
        resolve_run_commands(
        &args.selection.command_names,
        args.selection.effective_start(),
        args.selection.effective_after_deploy(),
    );

    if !commands_to_run.is_empty() {
        run_serve_commands(
            &paths.workspace_root,
            environment.runner.as_mut(),
            &commands_to_run,
            &args.selection.volume_specs,
            &runtime_serve_env(args.serve_port),
            Some(&environment.shipit_dir),
        )?;
    } else {
        console_print("No commands specified. Use `--command` to run a command.");
    }
    Ok(())
}

/// Port of tests/test_cli_after_deploy.py. The Python tests monkeypatch a
/// FakeRunner behind the typer CLI; here the same assertions drive
/// `resolve_run_commands` / `run_serve_commands` / `runtime_serve_env`
/// directly with a fake `Runner` (the CLI-level stderr behavior lives in
/// tests/run_cli.rs).
#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::path::PathBuf;

    use indexmap::IndexMap;
    use shipit_plan::{RunStep, Serve, Step};
    use shipit_providers::ProviderConfig;
    use shipit_run::Runner;

    use super::*;
    use crate::context::{resolve_environment, EnvironmentOptions};
    use crate::paths::resolve_project_paths;

    /// The FakeRunner from test_cli_after_deploy.py: records what the run
    /// pipeline asked of it.
    #[derive(Default)]
    struct FakeRunner {
        available_commands: HashSet<String>,
        checked: RefCell<Vec<String>>,
        calls: Vec<String>,
        volume_mappings: Vec<Option<IndexMap<String, String>>>,
        envs: Vec<Option<IndexMap<String, String>>>,
    }

    impl FakeRunner {
        fn with_commands(commands: &[&str]) -> Self {
            Self {
                available_commands: commands.iter().map(|c| (*c).to_owned()).collect(),
                ..Self::default()
            }
        }
    }

    impl Runner for FakeRunner {
        fn prepare_config(&mut self, config: ProviderConfig) -> ProviderConfig {
            config
        }

        fn prepare_build_steps(&self, steps: Vec<Step>) -> Vec<Step> {
            steps
        }

        fn build(&mut self, _serve: &Serve) -> anyhow::Result<()> {
            Ok(())
        }

        fn prepare(
            &mut self,
            _env: &IndexMap<String, String>,
            _prepare: &[RunStep],
        ) -> anyhow::Result<()> {
            Ok(())
        }

        fn has_serve_command(&self, command: &str) -> bool {
            self.checked.borrow_mut().push(command.to_owned());
            self.available_commands.contains(command)
        }

        fn run_serve_command(
            &mut self,
            command: &str,
            volume_mappings: Option<&IndexMap<String, String>>,
            env: Option<&IndexMap<String, String>>,
        ) -> anyhow::Result<()> {
            self.calls.push(command.to_owned());
            self.volume_mappings.push(volume_mappings.cloned());
            self.envs.push(env.cloned());
            Ok(())
        }

        fn get_serve_mount_path(&self, _name: &str) -> PathBuf {
            PathBuf::new()
        }

        fn as_any(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }

    #[test]
    fn run_runs_after_deploy_before_start() {
        let tmp = tempfile::tempdir().unwrap();
        // `run --start --after-deploy`.
        let commands = resolve_run_commands(&[], true, true);
        assert_eq!(commands, ["after_deploy", "start"]);

        let mut runner = FakeRunner::with_commands(&["start", "after_deploy"]);
        run_serve_commands(
            tmp.path(),
            &mut runner,
            &commands,
            &[],
            &IndexMap::new(),
            None,
        )
        .unwrap();

        assert_eq!(runner.calls, ["after_deploy", "start"]);
        assert_eq!(*runner.checked.borrow(), ["after_deploy", "start"]);
        assert_eq!(
            runner.volume_mappings,
            vec![Some(IndexMap::new()), Some(IndexMap::new())]
        );
    }

    #[test]
    fn run_skips_after_deploy_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let commands = resolve_run_commands(&[], true, true);

        let mut runner = FakeRunner::with_commands(&["start"]);
        run_serve_commands(
            tmp.path(),
            &mut runner,
            &commands,
            &[],
            &IndexMap::new(),
            None,
        )
        .unwrap();

        assert_eq!(runner.calls, ["start"]);
        assert_eq!(*runner.checked.borrow(), ["after_deploy", "start"]);
        assert_eq!(runner.volume_mappings, vec![Some(IndexMap::new())]);
    }

    #[test]
    fn run_runs_custom_commands_without_existence_checks() {
        let tmp = tempfile::tempdir().unwrap();
        // `run --no-start --command=prepare-db -c warm-cache`.
        let commands = resolve_run_commands(
            &["prepare-db".to_owned(), "warm-cache".to_owned()],
            false,
            false,
        );
        assert_eq!(commands, ["prepare-db", "warm-cache"]);

        let mut runner = FakeRunner::with_commands(&[]);
        run_serve_commands(
            tmp.path(),
            &mut runner,
            &commands,
            &[],
            &IndexMap::new(),
            None,
        )
        .unwrap();

        assert_eq!(runner.calls, ["prepare-db", "warm-cache"]);
        assert!(runner.checked.borrow().is_empty());
        assert_eq!(
            runner.volume_mappings,
            vec![Some(IndexMap::new()), Some(IndexMap::new())]
        );
    }

    /// Selection half of test_run_without_commands_prints_to_stderr; the
    /// stderr message itself is asserted in tests/run_cli.rs.
    #[test]
    fn run_without_commands_selects_nothing() {
        assert!(resolve_run_commands(&[], false, false).is_empty());
    }

    #[test]
    fn run_loads_volume_mappings_from_json() {
        let tmp = tempfile::tempdir().unwrap();
        let shipit_dir = tmp.path().join(".shipit");
        let mappings_dir = shipit_dir.join("volumes");
        std::fs::create_dir_all(&mappings_dir).unwrap();
        std::fs::write(
            mappings_dir.join("mappings.json"),
            "{\n  \"wp-content\": \"/app/wp-content\"\n}\n",
        )
        .unwrap();

        let mut runner = FakeRunner::with_commands(&["start"]);
        run_serve_commands(
            tmp.path(),
            &mut runner,
            &["start".to_owned()],
            &[],
            &IndexMap::new(),
            Some(&shipit_dir),
        )
        .unwrap();

        assert_eq!(runner.calls, ["start"]);
        assert_eq!(
            runner.volume_mappings,
            vec![Some(IndexMap::from([(
                "wp-content".to_owned(),
                "/app/wp-content".to_owned()
            )]))]
        );
    }

    #[test]
    fn run_merges_cli_volume_mappings() {
        let tmp = tempfile::tempdir().unwrap();
        let shipit_dir = tmp.path().join(".shipit");
        let mappings_dir = shipit_dir.join("volumes");
        std::fs::create_dir_all(&mappings_dir).unwrap();
        std::fs::write(
            mappings_dir.join("mappings.json"),
            "{\n  \"wp-content\": \"/app/wp-content\"\n}\n",
        )
        .unwrap();

        let mut runner = FakeRunner::with_commands(&["start"]);
        run_serve_commands(
            tmp.path(),
            &mut runner,
            &["start".to_owned()],
            &[
                "uploads:/app/uploads".to_owned(),
                "wp-content:/app/override".to_owned(),
            ],
            &IndexMap::new(),
            Some(&shipit_dir),
        )
        .unwrap();

        assert_eq!(runner.calls, ["start"]);
        assert_eq!(
            runner.volume_mappings,
            vec![Some(IndexMap::from([
                ("wp-content".to_owned(), "/app/override".to_owned()),
                ("uploads".to_owned(), "/app/uploads".to_owned()),
            ]))]
        );
    }

    /// test_run_passes_wasmer_registry_to_runner: run() feeds
    /// --wasmer-registry into resolve_environment; the registry must land
    /// on the constructed WasmerRunner.
    #[test]
    fn run_passes_wasmer_registry_to_runner() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = resolve_project_paths(tmp.path(), None).unwrap();
        let mut environment = resolve_environment(
            &paths,
            &EnvironmentOptions {
                wasmer: true,
                wasmer_registry: Some("wasmer.io".to_owned()),
                ..Default::default()
            },
        )
        .unwrap();
        let runner = environment
            .runner
            .as_any()
            .downcast_mut::<shipit_run::wasmer::WasmerRunner>()
            .expect("wasmer runner");
        assert_eq!(runner.wasmer_registry.as_deref(), Some("wasmer.io"));
    }

    #[test]
    fn run_passes_serve_port_to_runner_env() {
        let tmp = tempfile::tempdir().unwrap();
        // `run --start --serve-port=12345`.
        let env = runtime_serve_env(Some(12345));
        assert_eq!(
            env,
            IndexMap::from([("PORT".to_owned(), "12345".to_owned())])
        );

        let mut runner = FakeRunner::with_commands(&["start"]);
        run_serve_commands(
            tmp.path(),
            &mut runner,
            &["start".to_owned()],
            &[],
            &env,
            None,
        )
        .unwrap();

        assert_eq!(runner.calls, ["start"]);
        assert_eq!(runner.envs, vec![Some(env)]);
    }

    #[test]
    fn run_uses_host_port_env_when_serve_port_missing() {
        std::env::set_var("PORT", "23456");
        let env = runtime_serve_env(None);
        std::env::remove_var("PORT");
        assert_eq!(
            env,
            IndexMap::from([("PORT".to_owned(), "23456".to_owned())])
        );
    }
}
