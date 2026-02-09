//! Auto command - detect, generate, build, and serve

use crate::cli::{args::BuildArgs, output::Output};
use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

/// Detect, generate, build, and serve in one command
#[derive(Args, Debug)]
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

    #[command(flatten)]
    pub build_args: BuildArgs,
}

impl AutoCommand {
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

            let (ctx, serve_ctx) = crate::starlark::evaluate_shipit_file(&shipit_path)
                .with_context(|| format!("Failed to evaluate {}", shipit_path.display()))?;

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

                let mut env = serve_ctx.env.clone();
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
            let (ctx, serve_ctx) = crate::starlark::evaluate_shipit_file(&shipit_path)
                .with_context(|| format!("Failed to evaluate {}", shipit_path.display()))?;

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
            let command_name = serve
                .commands
                .keys()
                .next()
                .context("No commands defined")?
                .as_str();

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
