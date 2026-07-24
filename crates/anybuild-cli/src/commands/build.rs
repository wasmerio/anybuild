//! `anybuild build` (port of cli.py's build command).

use std::path::{Path, PathBuf};

use anybuild_build::local::ui::console_print;
use anybuild_plan::Step;
use anyhow::{bail, Result};
use indexmap::IndexMap;

use crate::args::{ProjectArgs, WasmerConnArgs};
use crate::context::{resolve_project_context, CommandOverrides, EnvironmentOptions};
use crate::paths::ProjectPaths;
use crate::volumes::build_volumes;

#[derive(clap::Args, Debug, Clone, Default)]
pub struct BuildArgs {
    #[command(flatten)]
    pub project: ProjectArgs,
    /// The path to the Anybuild file (defaults to Anybuild or Anybuild.<subdir>).
    #[arg(long, alias = "shipit-path")]
    pub anybuild_path: Option<PathBuf>,
    /// The start command to use (overwrites the default)
    #[arg(long)]
    pub start_command: Option<String>,
    /// The install command to use (overwrites the default)
    #[arg(long)]
    pub install_command: Option<String>,
    /// The build command to use (overwrites the default)
    #[arg(long)]
    pub build_command: Option<String>,
    /// Use Wasmer to build and package the project.
    #[arg(long)]
    pub wasmer: bool,
    /// Run the prepare command after building (defaults to True).
    #[arg(long)]
    pub skip_prepare: bool,
    #[command(flatten)]
    pub wasmer_conn: WasmerConnArgs,
    /// Use Docker to build the project.
    #[arg(long)]
    pub docker: bool,
    /// Use a specific Docker client (such as depot, podman, etc.)
    #[arg(long)]
    pub docker_client: Option<String>,
    /// Additional options to pass to the Docker client.
    #[arg(long)]
    pub docker_opts: Option<String>,
    /// Skip Docker if the build can be done safely locally (only copy commands).
    #[arg(long, overrides_with = "no_skip_docker_if_safe_build")]
    pub skip_docker_if_safe_build: bool,
    #[arg(long = "no-skip-docker-if-safe-build", hide = true)]
    pub no_skip_docker_if_safe_build: bool,
    /// The environment to use (defaults to `.env`, it will use .env.<env_name> if provided)
    #[arg(long)]
    pub env_name: Option<String>,
    /// The port to use (defaults to 8080).
    #[arg(long)]
    pub serve_port: Option<i64>,
    /// Use a specific provider to build the project.
    #[arg(long)]
    pub provider: Option<String>,
    /// The JSON content to use as input.
    #[arg(long)]
    pub config: Option<String>,
}

impl BuildArgs {
    /// typer's `--skip-docker-if-safe-build/--no-...` pair, default True.
    pub fn effective_skip_docker_if_safe_build(&self) -> bool {
        self.skip_docker_if_safe_build || !self.no_skip_docker_if_safe_build
    }
}

/// Port of `load_env_files`: layer `.env` (and `.env.<env_name>`) from the
/// workspace root and the app subdir into `target`.
pub fn load_env_files(
    paths: &ProjectPaths,
    env_name: Option<&str>,
    target: &mut IndexMap<String, String>,
) {
    let mut env_dirs: Vec<&Path> = vec![&paths.workspace_root];
    if paths.subdir.is_some() {
        env_dirs.push(&paths.app_path);
    }

    let mut env_names: Vec<String> = vec![".env".to_owned()];
    if let Some(env_name) = env_name {
        env_names.push(format!(".env.{env_name}"));
    }

    for env_dir in env_dirs {
        for env_file_name in &env_names {
            let env_file = env_dir.join(env_file_name);
            if env_file.exists() {
                if let Ok(iter) = dotenvy::from_path_iter(&env_file) {
                    for item in iter.flatten() {
                        target.insert(item.0, item.1);
                    }
                }
            }
        }
    }
}

pub fn run(args: BuildArgs) -> Result<()> {
    let overrides = CommandOverrides {
        start_command: args.start_command.clone(),
        install_command: args.install_command.clone(),
        build_command: args.build_command.clone(),
        serve_port: args.serve_port,
        use_provider: args.provider.clone(),
        config: args.config.clone(),
    };

    let mut path = args.project.path.clone();
    let mut subdir = args.project.subdir.clone();
    let mut skip_docker_if_safe_build = args.effective_skip_docker_if_safe_build();
    let mut env_options = EnvironmentOptions {
        wasmer: args.wasmer,
        wasmer_bin: args.wasmer_conn.wasmer_bin.clone(),
        wasmer_registry: args.wasmer_conn.wasmer_registry.clone(),
        wasmer_token: args.wasmer_conn.wasmer_token.clone(),
        docker: args.docker,
        docker_client: args.docker_client.clone(),
        docker_opts: args.docker_opts.clone(),
    };

    loop {
        let mut context = resolve_project_context(
            &path,
            subdir.as_deref(),
            args.anybuild_path.as_deref(),
            &overrides,
            &env_options,
        )?;

        let env: IndexMap<String, String> = IndexMap::from([
            ("PATH".to_owned(), String::new()),
            (
                "COLORTERM".to_owned(),
                std::env::var("COLORTERM").unwrap_or_default(),
            ),
            (
                "LSCOLORS".to_owned(),
                std::env::var("LSCOLORS").unwrap_or_else(|_| "0".to_owned()),
            ),
            (
                "LS_COLORS".to_owned(),
                std::env::var("LS_COLORS").unwrap_or_else(|_| "0".to_owned()),
            ),
            (
                "CLICOLOR".to_owned(),
                std::env::var("CLICOLOR").unwrap_or_else(|_| "0".to_owned()),
            ),
        ]);

        if skip_docker_if_safe_build && !context.serve.build.is_empty() {
            // If it doesn't have a run step, then it's safe to skip Docker
            // and run all the steps locally.
            let has_run = context
                .serve
                .build
                .iter()
                .any(|step| matches!(step, Step::Run(_)));
            if !has_run {
                console_print(
                    "ℹ️ Building locally instead of Docker to speed up the build, as all commands are safe to run locally",
                );
                // Python recurses into build() with the resolved paths,
                // skip_docker_if_safe_build=False, and docker turned off
                // (wasmer flags are kept).
                path = context.paths.workspace_root.clone();
                subdir = context.paths.subdir.clone();
                skip_docker_if_safe_build = false;
                env_options.docker = false;
                env_options.docker_client = None;
                env_options.docker_opts = None;
                continue;
            }
        }

        let mut serve_env = context.serve.env.take().unwrap_or_default();
        load_env_files(&context.paths, args.env_name.as_deref(), &mut serve_env);
        context.serve.env = Some(serve_env);

        if context
            .serve
            .commands
            .get("start")
            .map(|s| s.is_empty())
            .unwrap_or(true)
        {
            bail!("No start command could be found, please provide a start command");
        }

        // Prepare the build steps (sometimes the runners need to adapt the
        // dependencies).
        let build_steps = context
            .runner
            .prepare_build_steps(context.serve.build.clone());

        // Build and serve
        context.build_backend.borrow_mut().build(
            &context.serve.name,
            &env,
            context.serve.mounts.as_deref().unwrap_or(&[]),
            &build_steps,
        )?;
        build_volumes(
            &context.paths.workspace_root,
            &context.serve,
            Some(&context.anybuild_dir),
        )?;
        context.runner.build(&context.serve)?;
        let has_prepare = context
            .serve
            .prepare
            .as_ref()
            .map(|p| !p.is_empty())
            .unwrap_or(false);
        if has_prepare && !args.skip_prepare {
            console_print("\nRunning prepare step");
            let prepare = context.serve.prepare.clone().unwrap_or_default();
            context.runner.prepare(&env, &prepare)?;
        }
        return Ok(());
    }
}
