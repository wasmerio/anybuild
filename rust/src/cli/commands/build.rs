//! Build command - execute build

use crate::cli::{args::BuildArgs, output::Output};

use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

/// Build the project
#[derive(Args, Debug)]
#[command(after_help = "EXAMPLES:\n  \
    shipit build                # Build current directory\n  \
    shipit build my-app         # Build 'my-app' directory\n  \
    shipit build --clean        # Clean and rebuild\n  \
    shipit build --docker       # Use Docker backend\n  \
    shipit build --wasmer       # Use Wasmer")]
pub struct BuildCommand {
    /// Path to project directory
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Path to Shipit file (defaults to Shipit in the project path)
    #[arg(long)]
    pub shipit_path: Option<PathBuf>,

    /// JSON configuration content to override provider config
    #[arg(long)]
    pub config: Option<String>,

    /// Override the start command
    #[arg(long)]
    pub start_command: Option<String>,

    /// Override the install command
    #[arg(long)]
    pub install_command: Option<String>,

    /// Override the build command
    #[arg(long)]
    pub build_command: Option<String>,

    /// Use Wasmer to build and serve the project
    #[arg(long, overrides_with = "no_wasmer")]
    pub wasmer: bool,

    /// Do not use Wasmer
    #[arg(long = "no-wasmer", overrides_with = "wasmer")]
    pub no_wasmer: bool,

    /// Skip running prepare steps
    #[arg(long, overrides_with = "no_skip_prepare")]
    pub skip_prepare: bool,

    /// Do not skip prepare steps
    #[arg(long = "no-skip-prepare", overrides_with = "skip_prepare")]
    pub no_skip_prepare: bool,

    /// Path to the Wasmer binary
    #[arg(long)]
    pub wasmer_bin: Option<PathBuf>,

    /// Wasmer registry URL
    #[arg(long)]
    pub wasmer_registry: Option<String>,

    /// Wasmer token for authentication
    #[arg(long)]
    pub wasmer_token: Option<String>,

    /// Use a specific Docker client (such as depot, podman, etc.)
    #[arg(long)]
    pub docker_client: Option<String>,

    /// Additional options to pass to the Docker client
    #[arg(long)]
    pub docker_opts: Option<String>,

    /// Skip Docker if the build can be done safely locally (only copy commands)
    #[arg(long, overrides_with = "no_skip_docker_if_safe_build")]
    pub skip_docker_if_safe_build: bool,

    /// Do not skip Docker even if build is safe
    #[arg(
        long = "no-skip-docker-if-safe-build",
        overrides_with = "skip_docker_if_safe_build"
    )]
    pub no_skip_docker_if_safe_build: bool,

    /// Environment name to use (defaults to `.env`, will use `.env.<env_name>` if provided)
    #[arg(long)]
    pub env_name: Option<String>,

    /// Port to use for serving (defaults to 8080)
    #[arg(long)]
    pub serve_port: Option<u16>,

    /// Override detected provider
    #[arg(long)]
    pub provider: Option<String>,

    #[command(flatten)]
    pub build_args: BuildArgs,
}

impl BuildCommand {
    /// Get the effective value of wasmer flag
    pub fn should_use_wasmer(&self) -> bool {
        self.wasmer && !self.no_wasmer
    }

    /// Get the effective value of skip_prepare flag
    pub fn should_skip_prepare(&self) -> bool {
        self.skip_prepare && !self.no_skip_prepare
    }

    /// Get the effective value of skip_docker_if_safe_build flag (defaults to true)
    pub fn should_skip_docker_if_safe(&self) -> bool {
        !self.no_skip_docker_if_safe_build
    }

    /// Execute the build command
    pub fn execute(&self, output: &Output) -> Result<()> {
        let start = std::time::Instant::now();
        let shipit_path = crate::utils::path::resolve_shipit_path_with_override(
            &self.path,
            self.shipit_path.as_deref(),
        );

        output.step("📋", "Loading build plan...");

        // Check if file exists
        if !shipit_path.exists() {
            anyhow::bail!("Shipit file not found at {}", shipit_path.display());
        }

        // Detect provider config from project directory
        let project_dir = shipit_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let registry = crate::providers::ProviderRegistry::with_defaults();
        let provider_config = registry.detect_config(project_dir).unwrap_or_default();

        // Evaluate Shipit file
        let (ctx, mut serve) = crate::starlark::evaluate_shipit_file(&shipit_path, provider_config)
            .with_context(|| format!("Failed to evaluate {}", shipit_path.display()))?;

        // Load environment variables from .env files
        let env_path = project_dir.join(".env");
        if let Ok(env_vars) = crate::config::load_env_to_map(&env_path) {
            serve.env.extend(env_vars);
        }

        if let Some(ref env_name) = self.env_name {
            let env_path = project_dir.join(format!(".env.{}", env_name));
            if let Ok(env_vars) = crate::config::load_env_to_map(&env_path) {
                serve.env.extend(env_vars);
            }
        }

        output.success("Plan loaded");

        // Clean build directory if requested
        if self.build_args.clean {
            output.blank();
            output.step("🧹", "Cleaning build directory...");

            let src_dir = shipit_path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .canonicalize()
                .context("Failed to resolve project path")?;

            let build_dir = src_dir.join(".shipit");
            if build_dir.exists() {
                std::fs::remove_dir_all(&build_dir).context("Failed to remove build directory")?;
                output.success("Build directory cleaned");
            }
        }

        // Resolve build steps from references
        output.blank();
        output.step("🔍", "Resolving build steps...");

        let steps: Result<Vec<_>> = serve
            .build
            .iter()
            .map(|step_ref| {
                ctx.get_step(step_ref)
                    .with_context(|| format!("Failed to resolve step: {}", step_ref))
                    .cloned()
            })
            .collect();
        let steps = steps?;

        output.success(format!("Resolved {} build steps", steps.len()));

        if steps.is_empty() {
            output.warning("No build steps defined");
            return Ok(());
        }

        // Create backend
        output.blank();
        output.step("🔨", "Building project...");

        let src_dir = shipit_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));

        let src_dir = if src_dir.exists() {
            src_dir
                .canonicalize()
                .context("Failed to resolve project path")?
        } else {
            std::env::current_dir().context("Failed to get current directory")?
        };

        let assets_path = src_dir.join(".shipit").join("assets");

        let mut backend: Box<dyn crate::builders::BuildBackend> = if self.build_args.is_docker() {
            output.info("Using Docker build backend");
            let docker_client = std::env::var("DOCKER_HOST").ok();
            let docker_opts = self.build_args.backend.as_ref().and_then(|b| {
                if b == "docker" {
                    None
                } else {
                    Some(b.clone())
                }
            });
            Box::new(crate::builders::DockerBuildBackend::new(
                src_dir.clone(),
                assets_path,
                docker_client,
                docker_opts,
            ))
        } else {
            output.info("Using local build backend");
            Box::new(crate::builders::LocalBuildBackend::new(
                src_dir.clone(),
                assets_path,
            ))
        };

        // Resolve mounts
        let _mounts: Result<Vec<_>> = serve
            .mounts
            .iter()
            .map(|mount_ref| {
                ctx.get_mount(mount_ref)
                    .with_context(|| format!("Failed to resolve mount: {}", mount_ref))
                    .cloned()
            })
            .collect();
        let _mounts = _mounts?;

        // Execute build
        // Initialize environment like Python CLI - start with minimal vars
        // Command::new() inherits parent environment, these just set defaults
        let mut env = std::collections::HashMap::new();
        env.insert("PATH".to_string(), String::new());
        env.insert(
            "COLORTERM".to_string(),
            std::env::var("COLORTERM").unwrap_or_default(),
        );
        env.insert(
            "LSCOLORS".to_string(),
            std::env::var("LSCOLORS").unwrap_or_else(|_| "0".to_string()),
        );
        env.insert(
            "LS_COLORS".to_string(),
            std::env::var("LS_COLORS").unwrap_or_else(|_| "0".to_string()),
        );
        env.insert(
            "CLICOLOR".to_string(),
            std::env::var("CLICOLOR").unwrap_or_else(|_| "0".to_string()),
        );

        let total_steps = steps.len();
        let pb = output.progress_bar(total_steps as u64, "Building...");

        for (i, step) in steps.iter().enumerate() {
            pb.set_position(i as u64);
            pb.set_message(format!("Step {}/{}", i + 1, total_steps));

            backend
                .execute_step(step, &mut env)
                .with_context(|| format!("Failed to execute build step {}", i + 1))?;
        }

        pb.finish_with_message("Build complete");

        let duration = start.elapsed();
        output.blank();
        output.success(format!(
            "Built {} steps in {}",
            total_steps,
            crate::cli::output::format_duration(duration)
        ));

        // Show artifact location
        let artifact_dir = backend.get_artifact_mount_path("output");
        output.info(format!("Artifacts: {}", artifact_dir.display()));

        Ok(())
    }
}
