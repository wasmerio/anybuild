//! Serve command - run application

use crate::cli::{args::ServeArgs, output::Output};
use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

/// Serve the built project
#[derive(Args, Debug)]
#[command(after_help = "EXAMPLES:\n  \
    shipit serve                    # Serve current directory\n  \
    shipit serve my-app             # Serve 'my-app' directory\n  \
    shipit serve --wasmer           # Use Wasmer runner\n  \
    shipit serve --start            # Run start command\n  \
    shipit serve --wasmer-deploy    # Deploy to Wasmer Edge")]
pub struct ServeCommand {
    /// Path to project directory
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Path to Shipit file (defaults to Shipit in the project path)
    #[arg(long)]
    pub shipit_path: Option<PathBuf>,

    /// Use Docker backend for artifacts
    #[arg(long)]
    pub docker: bool,

    /// Path to the Wasmer binary
    #[arg(long)]
    pub wasmer_bin: Option<PathBuf>,

    /// Use a specific Docker client (such as depot, podman, etc.)
    #[arg(long)]
    pub docker_client: Option<String>,

    /// Additional options to pass to the Docker client
    #[arg(long)]
    pub docker_opts: Option<String>,

    /// Deploy the project to Wasmer
    #[arg(long, overrides_with = "no_wasmer_deploy")]
    pub wasmer_deploy: bool,

    /// Do not deploy to Wasmer
    #[arg(long = "no-wasmer-deploy", overrides_with = "wasmer_deploy")]
    pub no_wasmer_deploy: bool,

    /// Wasmer token for authentication
    #[arg(long)]
    pub wasmer_token: Option<String>,

    /// Wasmer registry URL
    #[arg(long)]
    pub wasmer_registry: Option<String>,

    /// Owner of the Wasmer app
    #[arg(long)]
    pub wasmer_app_owner: Option<String>,

    /// Name of the Wasmer app
    #[arg(long)]
    pub wasmer_app_name: Option<String>,

    /// Save the Wasmer build output to a JSON file
    #[arg(long)]
    pub wasmer_deploy_config: Option<PathBuf>,

    #[command(flatten)]
    pub serve_args: ServeArgs,
}

impl ServeCommand {
    /// Get the effective value of wasmer_deploy flag
    pub fn should_wasmer_deploy(&self) -> bool {
        self.wasmer_deploy && !self.no_wasmer_deploy
    }

    /// Execute the serve command
    pub async fn execute(&self, output: &Output) -> Result<()> {
        output.step("📋", "Loading serve configuration...");

        let shipit_path = crate::utils::path::resolve_shipit_path_with_override(
            &self.path,
            self.shipit_path.as_deref(),
        );

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
        let (ctx, serve_ctx) = crate::starlark::evaluate_shipit_file(&shipit_path, provider_config)
            .with_context(|| format!("Failed to evaluate {}", shipit_path.display()))?;

        output.success("Configuration loaded");

        // Convert starlark::ctx::Serve to types::serve::Serve
        // This is needed because Runner::build expects types::serve::Serve
        let serve = self
            .convert_serve(&ctx, &serve_ctx)
            .context("Failed to convert serve configuration")?;

        // Create backend
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

        let backend: std::sync::Arc<dyn crate::builders::BuildBackend> = if self.docker {
            output.info("Using Docker backend for artifacts");
            let docker_client = std::env::var("DOCKER_HOST").ok();
            std::sync::Arc::new(crate::builders::DockerBuildBackend::new(
                src_dir.clone(),
                assets_path,
                docker_client,
                None,
            ))
        } else {
            output.info("Using local backend for artifacts");
            std::sync::Arc::new(crate::builders::LocalBuildBackend::new(
                src_dir.clone(),
                assets_path,
            ))
        };

        // Create runner
        output.blank();
        output.step("🚀", "Starting server...");

        let mut runner: Box<dyn crate::runners::base::Runner> = if self.serve_args.is_wasmer() {
            output.info("Using Wasmer runner");
            Box::new(crate::runners::WasmerRunner::new(
                backend,
                src_dir.clone(),
                None,
                None,
                None,
            ))
        } else {
            output.info("Using local runner");
            Box::new(crate::runners::LocalRunner::new(backend, src_dir.clone()))
        };

        // Build runner (generate scripts/manifests)
        runner.build(&serve).context("Failed to build runner")?;

        // Run prepare steps if requested
        if self.serve_args.prepare {
            if let Some(prepare) = &serve.prepare {
                if !prepare.is_empty() {
                    output.blank();
                    output.step("⚙️", "Running prepare steps...");

                    let mut env = serve.env.clone().unwrap_or_default();

                    // Merge CLI environment variables
                    for (key, value) in self.serve_args.parse_env_vars()? {
                        env.insert(key, value);
                    }

                    runner
                        .prepare(&env, prepare)
                        .context("Failed to run prepare steps")?;
                    output.success("Prepare complete");
                }
            }
        }

        // Determine which command to run
        let command_name = if let Some(ref cmd) = self.serve_args.command {
            if !serve.commands.contains_key(cmd) {
                anyhow::bail!(
                    "Command '{}' not found. Available: {}",
                    cmd,
                    serve
                        .commands
                        .keys()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            cmd.as_str()
        } else if self.serve_args.start {
            if !serve.commands.contains_key("start") {
                anyhow::bail!(
                    "Command 'start' not found. Available: {}",
                    serve
                        .commands
                        .keys()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            "start"
        } else if serve.commands.contains_key("start") {
            "start"
        } else {
            serve
                .commands
                .keys()
                .next()
                .context("No commands defined in serve configuration")?
                .as_str()
        };

        output.blank();
        output.success(format!("Running command: {}", command_name));
        output.info("Press Ctrl+C to stop");
        output.blank();

        // Run serve command
        runner
            .run_serve_command(command_name)
            .context("Failed to run serve command")?;

        Ok(())
    }

    /// Convert starlark::ctx::Serve to types::serve::Serve
    fn convert_serve(
        &self,
        ctx: &crate::starlark::Ctx,
        serve_ctx: &crate::starlark::ctx::Serve,
    ) -> Result<crate::types::serve::Serve> {
        use anyhow::Context;

        // Resolve build steps
        let build_steps: Result<Vec<_>> = serve_ctx
            .build
            .iter()
            .map(|step_ref| {
                ctx.get_step(step_ref)
                    .with_context(|| format!("Failed to resolve build step: {}", step_ref))
                    .cloned()
            })
            .collect();
        let build_steps = build_steps?;

        // Resolve dependencies
        let deps: Result<Vec<_>> = serve_ctx
            .deps
            .iter()
            .map(|pkg_ref| {
                ctx.get_package(pkg_ref)
                    .with_context(|| format!("Failed to resolve package: {}", pkg_ref))
                    .cloned()
            })
            .collect();
        let deps = deps?;

        // Resolve mounts
        let mounts: Result<Vec<_>> = serve_ctx
            .mounts
            .iter()
            .map(|mount_ref| {
                ctx.get_mount(mount_ref)
                    .with_context(|| format!("Failed to resolve mount: {}", mount_ref))
                    .cloned()
            })
            .collect();
        let mounts = mounts?;

        // Resolve volumes
        let volumes: Result<Vec<_>> = serve_ctx
            .volumes
            .iter()
            .map(|vol_ref| {
                ctx.get_volume(vol_ref)
                    .with_context(|| format!("Failed to resolve volume: {}", vol_ref))
                    .cloned()
            })
            .collect();
        let volumes = volumes?;

        // Resolve services
        let services: Result<Vec<_>> = serve_ctx
            .services
            .iter()
            .map(|svc_ref| {
                ctx.get_service(svc_ref)
                    .with_context(|| format!("Failed to resolve service: {}", svc_ref))
                    .cloned()
            })
            .collect();
        let services = services?;

        // Resolve prepare steps (they are also step references)
        let prepare: Result<Vec<_>> = serve_ctx
            .prepare
            .iter()
            .map(|step_ref| {
                ctx.get_step(step_ref)
                    .with_context(|| format!("Failed to resolve prepare step: {}", step_ref))
                    .and_then(|step| {
                        // PrepareStep is a RunStep, so extract it
                        match step {
                            crate::types::Step::Run(run_step) => Ok(run_step.clone()),
                            _ => anyhow::bail!("Prepare step must be a run step, got: {:?}", step),
                        }
                    })
            })
            .collect();
        let prepare = prepare?;

        Ok(crate::types::serve::Serve {
            name: serve_ctx.name.clone(),
            provider: serve_ctx.provider.clone(),
            build: build_steps,
            deps,
            commands: serve_ctx.commands.clone(),
            cwd: serve_ctx.cwd.clone(),
            prepare: if prepare.is_empty() {
                None
            } else {
                Some(prepare)
            },
            workers: if serve_ctx.workers.is_empty() {
                None
            } else {
                Some(serve_ctx.workers.clone())
            },
            mounts: if mounts.is_empty() {
                None
            } else {
                Some(mounts)
            },
            volumes: if volumes.is_empty() {
                None
            } else {
                Some(volumes)
            },
            env: if serve_ctx.env.is_empty() {
                None
            } else {
                Some(serve_ctx.env.clone())
            },
            services: if services.is_empty() {
                None
            } else {
                Some(services)
            },
        })
    }
}
