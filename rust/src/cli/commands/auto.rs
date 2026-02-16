//! Auto command - detect, generate, build, and serve

use crate::cli::{args::BuildArgs, output::Output};
use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

/// Detect, generate, build, and serve in one command
#[derive(Args, Debug)]
#[command(after_help = "EXAMPLES:\n  \
    shipit                    # Auto-detect and serve current directory\n  \
    shipit my-app             # Auto-detect and serve 'my-app' directory\n  \
    shipit --skip-build       # Skip build step and just serve\n  \
    shipit --wasmer           # Use Wasmer runner instead of local\n  \
    shipit --provider nodejs  # Force specific provider")]
pub struct AutoCommand {
    /// Path to project directory
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Skip building
    #[arg(long)]
    pub skip_build: bool,

    /// Skip serving
    #[arg(long)]
    pub skip_serve: bool,

    /// Use Wasmer runner for serving
    #[arg(long)]
    pub wasmer: bool,

    /// Run the start command
    #[arg(long)]
    pub start: bool,

    /// JSON configuration content to override provider config
    #[arg(long)]
    pub config: Option<String>,

    /// Path to Shipit file (defaults to Shipit in the project path)
    #[arg(long)]
    pub shipit_path: Option<PathBuf>,

    /// Regenerate the Shipit file
    #[arg(long, overrides_with = "no_regenerate")]
    pub regenerate: bool,

    /// Do not regenerate the Shipit file
    #[arg(long = "no-regenerate", overrides_with = "regenerate")]
    pub no_regenerate: bool,

    /// Use a temporary Shipit file in the system temporary directory
    #[arg(long, overrides_with = "no_temp_shipit")]
    pub temp_shipit: bool,

    /// Do not use a temporary Shipit file
    #[arg(long = "no-temp-shipit", overrides_with = "temp_shipit")]
    pub no_temp_shipit: bool,

    /// Path to the Wasmer binary
    #[arg(long)]
    pub wasmer_bin: Option<PathBuf>,

    /// Wasmer token for authentication
    #[arg(long)]
    pub wasmer_token: Option<String>,

    /// Wasmer registry URL
    #[arg(long)]
    pub wasmer_registry: Option<String>,

    /// Deploy the project to Wasmer
    #[arg(long, overrides_with = "no_wasmer_deploy")]
    pub wasmer_deploy: bool,

    /// Do not deploy to Wasmer
    #[arg(long = "no-wasmer-deploy", overrides_with = "wasmer_deploy")]
    pub no_wasmer_deploy: bool,

    /// Save the Wasmer build output to a JSON file
    #[arg(long)]
    pub wasmer_deploy_config: Option<PathBuf>,

    /// Owner of the Wasmer app
    #[arg(long)]
    pub wasmer_app_owner: Option<String>,

    /// Name of the Wasmer app
    #[arg(long)]
    pub wasmer_app_name: Option<String>,

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
    #[arg(long = "no-skip-docker-if-safe-build", overrides_with = "skip_docker_if_safe_build")]
    pub no_skip_docker_if_safe_build: bool,

    /// Skip running prepare steps
    #[arg(long, overrides_with = "no_skip_prepare")]
    pub skip_prepare: bool,

    /// Do not skip prepare steps
    #[arg(long = "no-skip-prepare", overrides_with = "skip_prepare")]
    pub no_skip_prepare: bool,

    /// Override the install command
    #[arg(long)]
    pub install_command: Option<String>,

    /// Override the build command
    #[arg(long)]
    pub build_command: Option<String>,

    /// Override the start command
    #[arg(long)]
    pub start_command: Option<String>,

    /// Environment name to use (defaults to `.env`, will use `.env.<env_name>` if provided)
    #[arg(long)]
    pub env_name: Option<String>,

    /// Override detected provider
    #[arg(long)]
    pub provider: Option<String>,

    /// Port to use for serving (defaults to 8080)
    #[arg(long)]
    pub serve_port: Option<u16>,

    #[command(flatten)]
    pub build_args: BuildArgs,
}

impl AutoCommand {
    /// Get the effective value of regenerate flag
    pub fn should_regenerate(&self) -> bool {
        self.regenerate && !self.no_regenerate
    }

    /// Get the effective value of temp_shipit flag
    pub fn should_use_temp_shipit(&self) -> bool {
        self.temp_shipit && !self.no_temp_shipit
    }

    /// Get the effective value of wasmer_deploy flag
    pub fn should_wasmer_deploy(&self) -> bool {
        self.wasmer_deploy && !self.no_wasmer_deploy
    }

    /// Get the effective value of skip_docker_if_safe_build flag (defaults to true)
    pub fn should_skip_docker_if_safe(&self) -> bool {
        if self.no_skip_docker_if_safe_build {
            false
        } else {
            // Default to true if neither flag is set, or if skip flag is set
            !self.skip_docker_if_safe_build || self.skip_docker_if_safe_build
        }
    }

    /// Get the effective value of skip_prepare flag
    pub fn should_skip_prepare(&self) -> bool {
        self.skip_prepare && !self.no_skip_prepare
    }

    /// Execute the auto command
    pub async fn execute(&self, output: &Output) -> Result<()> {
        use std::time::Instant;
        let start = Instant::now();

        output.step("🔍", "Auto-detecting project type...");

        // Load configuration
        let config = crate::config::Config::load_layered(None).unwrap_or_default();
        let registry = crate::providers::ProviderRegistry::with_defaults();

        // Detect provider
        let pb = output.progress("Scanning project...");
        let provider = crate::generator::detect_provider(&self.path, &registry, &config)?;
        pb.finish_and_clear();
        output.success(format!("Detected: {}", provider.name()));

        // Generate Shipit file
        output.blank();
        output.step("📝", "Generating Shipit file...");

        let shipit_path = self.path.join("Shipit");

        if shipit_path.exists() {
            output.info("Shipit file already exists, using existing file");
        } else {
            let plan = provider.plan(&self.path)?;
            let content = crate::generator::generate_shipit_file(&self.path, &plan)
                .context("Failed to generate Shipit file content")?;

            std::fs::write(&shipit_path, content).with_context(|| {
                format!("Failed to write Shipit file to {}", shipit_path.display())
            })?;

            output.success(format!("Created: {}", shipit_path.display()));
        }

        // Build
        if !self.skip_build {
            output.blank();
            output.step("🔨", "Building project...");

            let provider_config = provider
                .provider_config(&self.path)
                .unwrap_or_default();

            let (ctx, mut serve_ctx) = crate::starlark::evaluate_shipit_file(
                &shipit_path,
                provider_config,
            )
                .with_context(|| format!("Failed to evaluate {}", shipit_path.display()))?;

            // Load environment variables from .env files
            let env_path = self.path.join(".env");
            if let Ok(env_vars) = crate::config::load_env_to_map(&env_path) {
                serve_ctx.env.extend(env_vars);
            }

            if let Some(ref env_name) = self.env_name {
                let env_path = self.path.join(format!(".env.{}", env_name));
                if let Ok(env_vars) = crate::config::load_env_to_map(&env_path) {
                    serve_ctx.env.extend(env_vars);
                }
            }

            // Resolve build steps
            let mut steps = Vec::new();
            for step_ref in &serve_ctx.build {
                let step = ctx
                    .get_step(step_ref)
                    .with_context(|| format!("Failed to resolve step: {}", step_ref))?;
                steps.push(step.clone());
            }

            if !steps.is_empty() {
                let src_dir = self
                    .path
                    .canonicalize()
                    .context("Failed to resolve project path")?;
                let assets_path = src_dir.join(".shipit").join("assets");

                let mut backend: Box<dyn crate::builders::BuildBackend> = if self.build_args.docker
                {
                    output.info("Using Docker backend");
                    let docker_client = std::env::var("DOCKER_HOST").ok();
                    Box::new(crate::builders::DockerBuildBackend::new(
                        src_dir.clone(),
                        assets_path,
                        docker_client,
                        None,
                    ))
                } else {
                    output.info("Using local backend");
                    Box::new(crate::builders::LocalBuildBackend::new(
                        src_dir,
                        assets_path,
                    ))
                };

                // Initialize environment like Python CLI - start with minimal vars
                // Command::new() inherits parent environment, these just set defaults
                let mut env = std::collections::HashMap::new();
                env.insert("PATH".to_string(), String::new());
                env.insert("COLORTERM".to_string(), std::env::var("COLORTERM").unwrap_or_default());
                env.insert("LSCOLORS".to_string(), std::env::var("LSCOLORS").unwrap_or_else(|_| "0".to_string()));
                env.insert("LS_COLORS".to_string(), std::env::var("LS_COLORS").unwrap_or_else(|_| "0".to_string()));
                env.insert("CLICOLOR".to_string(), std::env::var("CLICOLOR").unwrap_or_else(|_| "0".to_string()));

                let pb = output.progress_bar(steps.len() as u64, "Building...");
                for (i, step) in steps.iter().enumerate() {
                    pb.set_position(i as u64);
                    pb.set_message(format!("Step {}/{}", i + 1, steps.len()));
                    backend
                        .execute_step(step, &mut env)
                        .with_context(|| format!("Failed to execute step: {:?}", step))?;
                }
                pb.finish_with_message("Build complete");
            } else {
                output.info("No build steps to execute");
            }
        }

        // Serve
        if !self.skip_serve {
            output.blank();
            output.step("🚀", "Starting server...");

            // Re-evaluate to get fresh context
            let provider_config = provider
                .provider_config(&self.path)
                .unwrap_or_default();

            let (ctx, mut serve_ctx) = crate::starlark::evaluate_shipit_file(
                &shipit_path,
                provider_config,
            )
                .with_context(|| format!("Failed to evaluate {}", shipit_path.display()))?;

            // Load environment variables from .env files
            let env_path = self.path.join(".env");
            if let Ok(env_vars) = crate::config::load_env_to_map(&env_path) {
                serve_ctx.env.extend(env_vars);
            }

            if let Some(ref env_name) = self.env_name {
                let env_path = self.path.join(format!(".env.{}", env_name));
                if let Ok(env_vars) = crate::config::load_env_to_map(&env_path) {
                    serve_ctx.env.extend(env_vars);
                }
            }

            // Convert serve configuration
            let serve = self
                .convert_serve(&ctx, &serve_ctx)
                .context("Failed to convert serve configuration")?;

            // Create runner
            let src_dir = self
                .path
                .canonicalize()
                .context("Failed to resolve project path")?;
            let assets_path = src_dir.join(".shipit").join("assets");

            let backend: std::sync::Arc<dyn crate::builders::BuildBackend> =
                if self.build_args.docker {
                    let docker_client = std::env::var("DOCKER_HOST").ok();
                    std::sync::Arc::new(crate::builders::DockerBuildBackend::new(
                        src_dir.clone(),
                        assets_path,
                        docker_client,
                        None,
                    ))
                } else {
                    std::sync::Arc::new(crate::builders::LocalBuildBackend::new(
                        src_dir.clone(),
                        assets_path,
                    ))
                };

            let mut runner: Box<dyn crate::runners::base::Runner> = if self.wasmer {
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
                Box::new(crate::runners::LocalRunner::new(backend, src_dir))
            };

            runner.build(&serve).context("Failed to build runner")?;

            // Determine command
            let command_name = if self.start || serve.commands.contains_key("start") {
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
            } else {
                serve
                    .commands
                    .keys()
                    .next()
                    .context("No commands defined")?
                    .as_str()
            };

            output.blank();
            output.success(format!("✓ Ready in {:.2}s", start.elapsed().as_secs_f64()));
            output.success(format!("Running: {}", command_name));
            output.info("Press Ctrl+C to stop");
            output.blank();

            runner
                .run_serve_command(command_name)
                .context("Failed to run serve command")?;
        } else {
            output.blank();
            output.success(format!(
                "✓ Complete in {:.2}s",
                start.elapsed().as_secs_f64()
            ));
        }

        Ok(())
    }

    /// Convert starlark::ctx::Serve to types::serve::Serve
    fn convert_serve(
        &self,
        ctx: &crate::starlark::Ctx,
        serve_ctx: &crate::starlark::ctx::Serve,
    ) -> Result<crate::types::serve::Serve> {
        let build_steps: Result<Vec<_>> = serve_ctx
            .build
            .iter()
            .map(|r| ctx.get_step(r).cloned())
            .collect();
        let deps: Result<Vec<_>> = serve_ctx
            .deps
            .iter()
            .map(|r| ctx.get_package(r).cloned())
            .collect();
        let mounts: Result<Vec<_>> = serve_ctx
            .mounts
            .iter()
            .map(|r| ctx.get_mount(r).cloned())
            .collect();
        let services: Result<Vec<_>> = serve_ctx
            .services
            .iter()
            .map(|r| ctx.get_service(r).cloned())
            .collect();
        let prepare: Result<Vec<_>> = serve_ctx
            .prepare
            .iter()
            .map(|r| {
                ctx.get_step(r).and_then(|step| match step {
                    crate::types::Step::Run(run_step) => Ok(run_step.clone()),
                    _ => anyhow::bail!("Prepare step must be a run step"),
                })
            })
            .collect();

        Ok(crate::types::serve::Serve {
            name: serve_ctx.name.clone(),
            provider: serve_ctx.provider.clone(),
            build: build_steps?,
            deps: deps?,
            commands: serve_ctx.commands.clone(),
            cwd: serve_ctx.cwd.clone(),
            prepare: {
                let p = prepare?;
                if p.is_empty() {
                    None
                } else {
                    Some(p)
                }
            },
            workers: if serve_ctx.workers.is_empty() {
                None
            } else {
                Some(serve_ctx.workers.clone())
            },
            mounts: {
                let m = mounts?;
                if m.is_empty() {
                    None
                } else {
                    Some(m)
                }
            },
            volumes: None,
            env: if serve_ctx.env.is_empty() {
                None
            } else {
                Some(serve_ctx.env.clone())
            },
            services: {
                let s = services?;
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            },
        })
    }
}
