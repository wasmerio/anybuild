//! Serve command - run application

use crate::cli::{args::ServeArgs, output::Output};
use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

/// Serve the built project
#[derive(Args, Debug)]
pub struct ServeCommand {
    /// Path to Shipit file
    #[arg(default_value = "Shipit")]
    pub shipit_path: PathBuf,

    /// Use Docker backend for artifacts
    #[arg(long)]
    pub docker: bool,

    #[command(flatten)]
    pub serve_args: ServeArgs,
}

impl ServeCommand {
    /// Execute the serve command
    pub async fn execute(&self, output: &Output) -> Result<()> {
        output.step("📋", "Loading serve configuration...");

        // Check if file exists
        if !self.shipit_path.exists() {
            anyhow::bail!("Shipit file not found at {}", self.shipit_path.display());
        }

        // Evaluate Shipit file
        let (ctx, serve_ctx) = crate::starlark::evaluate_shipit_file(&self.shipit_path)
            .with_context(|| format!("Failed to evaluate {}", self.shipit_path.display()))?;

        output.success("Configuration loaded");

        // Convert starlark::ctx::Serve to types::serve::Serve
        // This is needed because Runner::build expects types::serve::Serve
        let serve = self
            .convert_serve(&ctx, &serve_ctx)
            .context("Failed to convert serve configuration")?;

        // Create backend
        let src_dir = self
            .shipit_path
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
