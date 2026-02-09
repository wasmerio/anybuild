//! Plan command - show build plan

use crate::cli::output::Output;

use anyhow::{Context, Result};
use clap::{Args, ValueEnum};
use std::path::PathBuf;

/// Show the build plan without executing
#[derive(Args, Debug)]
pub struct PlanCommand {
    /// Path to Shipit file
    #[arg(default_value = "Shipit")]
    pub shipit_path: PathBuf,

    /// Output format
    #[arg(long, short, value_enum, default_value = "json")]
    pub format: OutputFormat,

    /// Pretty print output
    #[arg(long, short)]
    pub pretty: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Json,
    Yaml,
    Toml,
}

impl PlanCommand {
    /// Execute the plan command
    pub fn execute(&self, output: &Output) -> Result<()> {
        output.step("📋", "Loading plan...");

        // Check if file exists
        if !self.shipit_path.exists() {
            anyhow::bail!("Shipit file not found at {}", self.shipit_path.display());
        }

        // Evaluate Shipit file
        let (ctx, serve) = crate::starlark::evaluate_shipit_file(&self.shipit_path)
            .with_context(|| format!("Failed to evaluate {}", self.shipit_path.display()))?;

        output.success("Plan loaded successfully");
        output.blank();

        // Create a serializable representation
        #[derive(serde::Serialize)]
        struct PlanOutput {
            serve: ServeOutput,
            packages: usize,
            steps: usize,
            mounts: usize,
            volumes: usize,
            services: usize,
        }

        #[derive(serde::Serialize)]
        struct ServeOutput {
            name: String,
            provider: String,
            build: Vec<String>,
            commands: std::collections::HashMap<String, String>,
            cwd: Option<String>,
            prepare: Vec<String>,
            workers: Vec<String>,
            env: std::collections::HashMap<String, String>,
        }

        let plan_output = PlanOutput {
            serve: ServeOutput {
                name: serve.name.clone(),
                provider: serve.provider.clone(),
                build: serve.build.clone(),
                commands: serve.commands.clone(),
                cwd: serve.cwd.clone(),
                prepare: serve.prepare.clone(),
                workers: serve.workers.clone(),
                env: serve.env.clone(),
            },
            packages: ctx.packages.len(),
            steps: ctx.steps.len(),
            mounts: ctx.mounts.len(),
            volumes: ctx.volumes.len(),
            services: ctx.services.len(),
        };

        // Format and display plan
        let formatted =
            match self.format {
                OutputFormat::Json => {
                    if self.pretty {
                        serde_json::to_string_pretty(&plan_output)
                            .context("Failed to serialize plan to JSON")?
                    } else {
                        serde_json::to_string(&plan_output)
                            .context("Failed to serialize plan to JSON")?
                    }
                }
                OutputFormat::Yaml => serde_yaml::to_string(&plan_output)
                    .context("Failed to serialize plan to YAML")?,
                OutputFormat::Toml => toml::to_string_pretty(&plan_output)
                    .context("Failed to serialize plan to TOML")?,
            };

        println!("{}", formatted);

        Ok(())
    }
}
