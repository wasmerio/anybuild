//! Build command - execute build

use crate::cli::{args::BuildArgs, output::Output};

use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

/// Build the project
#[derive(Args, Debug)]
pub struct BuildCommand {
    /// Path to Shipit file
    #[arg(default_value = "Shipit")]
    pub shipit_path: PathBuf,

    #[command(flatten)]
    pub build_args: BuildArgs,
}

impl BuildCommand {
    /// Execute the build command
    pub fn execute(&self, output: &Output) -> Result<()> {
        let start = std::time::Instant::now();

        output.step("📋", "Loading build plan...");

        // Check if file exists
        if !self.shipit_path.exists() {
            anyhow::bail!("Shipit file not found at {}", self.shipit_path.display());
        }

        // Evaluate Shipit file
        let (ctx, serve) = crate::starlark::evaluate_shipit_file(&self.shipit_path)
            .with_context(|| format!("Failed to evaluate {}", self.shipit_path.display()))?;

        output.success("Plan loaded");

        // Clean build directory if requested
        if self.build_args.clean {
            output.blank();
            output.step("🧹", "Cleaning build directory...");

            let src_dir = self
                .shipit_path
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
        let mut env = serve.env.clone();
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
