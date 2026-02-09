//! Deploy command - deploy to Wasmer Edge

use crate::cli::{args::DeployArgs, output::Output};
use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

/// Deploy to Wasmer Edge
#[derive(Args, Debug)]
pub struct DeployCommand {
    /// Path to Shipit file
    #[arg(default_value = "Shipit")]
    pub shipit_path: PathBuf,

    #[command(flatten)]
    pub deploy_args: DeployArgs,
}

impl DeployCommand {
    /// Execute the deploy command
    pub async fn execute(&self, output: &Output) -> Result<()> {
        output.step("🚀", "Deploying to Wasmer Edge...");

        // Check if file exists
        if !self.shipit_path.exists() {
            anyhow::bail!("Shipit file not found at {}", self.shipit_path.display());
        }

        // Evaluate Shipit file
        let (ctx, serve_ctx) = crate::starlark::evaluate_shipit_file(&self.shipit_path)
            .with_context(|| format!("Failed to evaluate {}", self.shipit_path.display()))?;

        output.success("Configuration loaded");

        // Convert serve configuration
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

        // Always use local backend for deployment
        let backend: std::sync::Arc<dyn crate::builders::BuildBackend> = std::sync::Arc::new(
            crate::builders::LocalBuildBackend::new(src_dir.clone(), assets_path),
        );

        // Create Wasmer runner
        output.blank();
        output.step("📦", "Building Wasmer manifest...");

        let registry = self
            .deploy_args
            .registry
            .clone()
            .or_else(|| std::env::var("WASMER_REGISTRY").ok());

        let token = self
            .deploy_args
            .token
            .clone()
            .or_else(|| std::env::var("WASMER_TOKEN").ok());

        let mut runner = crate::runners::WasmerRunner::new(
            backend,
            src_dir.clone(),
            registry.clone(),
            token.clone(),
            None, // Use default wasmer binary
        );

        // Build runner (generate manifest and scripts)
        // Note: Need to use Runner trait explicitly
        use crate::runners::base::Runner;
        runner
            .build(&serve)
            .context("Failed to build Wasmer runner")?;

        output.success("Manifest generated");

        // Deploy
        output.blank();
        output.step("🌍", "Publishing to Wasmer Edge...");

        if registry.is_none() {
            anyhow::bail!("Wasmer registry not configured. Set --registry or WASMER_REGISTRY");
        }

        if token.is_none() {
            anyhow::bail!("Wasmer token not configured. Set --token or WASMER_TOKEN");
        }

        let app_name = &self.deploy_args.app;

        runner
            .deploy(app_name)
            .context("Failed to deploy to Wasmer Edge")?;

        output.blank();
        output.success(format!("✓ Deployed: {}", app_name));

        if let Some(reg) = &registry {
            output.info(format!("Registry: {}", reg));
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
