use std::path::PathBuf;

use anybuild::BuildOptions;
use anyhow::Result;

use crate::args::{ProjectArgs, WasmerConnArgs};
use crate::commands::{client_with_render_options, execution, RenderOptions};
use crate::SharedProjectArgs;

#[derive(clap::Args, Debug, Clone, Default)]
pub struct BuildArgs {
    #[command(flatten)]
    pub project: ProjectArgs,
    #[arg(long, alias = "shipit-path")]
    pub anybuild_path: Option<PathBuf>,
    #[arg(long)]
    pub start_command: Option<String>,
    #[arg(long)]
    pub install_command: Option<String>,
    #[arg(long)]
    pub build_command: Option<String>,
    #[arg(long)]
    pub wasmer: bool,
    #[arg(long)]
    pub skip_prepare: bool,
    #[command(flatten)]
    pub wasmer_conn: WasmerConnArgs,
    #[arg(long)]
    pub docker: bool,
    #[arg(long)]
    pub docker_client: Option<String>,
    #[arg(long)]
    pub docker_opts: Option<String>,
    #[arg(long, overrides_with = "no_skip_docker_if_safe_build")]
    pub skip_docker_if_safe_build: bool,
    #[arg(long = "no-skip-docker-if-safe-build", hide = true)]
    pub no_skip_docker_if_safe_build: bool,
    #[arg(long)]
    pub env_name: Option<String>,
    #[arg(long)]
    pub serve_port: Option<i64>,
    #[arg(long)]
    pub provider: Option<String>,
    #[arg(long)]
    pub config: Option<String>,
    /// Include copy and working-directory operations in the build summary.
    #[arg(long)]
    pub show_detailed_steps: bool,
    /// Show the generated wasmer.toml and app.yaml contents.
    #[arg(long)]
    pub show_wasmer_files: bool,
}

impl BuildArgs {
    pub fn effective_skip_docker_if_safe_build(&self) -> bool {
        self.skip_docker_if_safe_build || !self.no_skip_docker_if_safe_build
    }
}

pub fn run(args: BuildArgs) -> Result<()> {
    let skip_docker_if_safe = args.effective_skip_docker_if_safe_build();
    let shared = SharedProjectArgs {
        path: args.project.path,
        subdir: args.project.subdir,
        install_command: args.install_command,
        build_command: args.build_command,
        start_command: args.start_command,
        provider: args.provider,
        config: args.config,
    };
    let (build_environment, runtime_environment) = execution(
        args.wasmer,
        args.wasmer_conn.wasmer_bin,
        args.wasmer_conn.wasmer_registry,
        args.wasmer_conn.wasmer_token,
        args.docker,
        args.docker_client,
        args.docker_opts,
    );
    client_with_render_options(
        &shared,
        args.serve_port,
        RenderOptions {
            show_detailed_steps: args.show_detailed_steps,
            show_wasmer_files: args.show_wasmer_files,
        },
    )?
    .build(BuildOptions {
        anybuild_path: args.anybuild_path,
        build_environment,
        runtime_environment,
        skip_prepare: args.skip_prepare,
        skip_docker_if_safe,
        env_name: args.env_name,
        ..Default::default()
    })?;
    Ok(())
}
