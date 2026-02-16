//! Plan command - show build plan

use crate::cli::output::Output;

use anyhow::{Context, Result};
use clap::{Args, ValueEnum};
use std::path::PathBuf;

/// Show the build plan without executing
#[derive(Args, Debug)]
#[command(after_help = "EXAMPLES:\n  \
    shipit plan                   # Show plan for current directory\n  \
    shipit plan my-app            # Show plan for 'my-app'\n  \
    shipit plan -f yaml           # Output in YAML format\n  \
    shipit plan -o plan.json      # Save to file\n  \
    shipit plan --pretty          # Pretty print output")]
pub struct PlanCommand {
    /// Path to project directory
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Path to Shipit file (defaults to Shipit in the project path)
    #[arg(long)]
    pub shipit_path: Option<PathBuf>,

    /// JSON configuration content to override provider config
    #[arg(long)]
    pub config: Option<String>,

    /// Output file path (defaults to stdout)
    #[arg(long, short = 'o', visible_alias = "out")]
    pub output: Option<PathBuf>,

    /// Use a temporary Shipit file in the system temporary directory
    #[arg(long, overrides_with = "no_temp_shipit")]
    pub temp_shipit: bool,

    /// Do not use a temporary Shipit file
    #[arg(long = "no-temp-shipit", overrides_with = "temp_shipit")]
    pub no_temp_shipit: bool,

    /// Regenerate the Shipit file
    #[arg(long, overrides_with = "no_regenerate")]
    pub regenerate: bool,

    /// Do not regenerate the Shipit file
    #[arg(long = "no-regenerate", overrides_with = "regenerate")]
    pub no_regenerate: bool,

    /// Use Wasmer to evaluate the project
    #[arg(long, overrides_with = "no_wasmer")]
    pub wasmer: bool,

    /// Do not use Wasmer
    #[arg(long = "no-wasmer", overrides_with = "wasmer")]
    pub no_wasmer: bool,

    /// Path to the Wasmer binary
    #[arg(long)]
    pub wasmer_bin: Option<PathBuf>,

    /// Wasmer registry URL
    #[arg(long)]
    pub wasmer_registry: Option<String>,

    /// Wasmer token for authentication
    #[arg(long)]
    pub wasmer_token: Option<String>,

    /// Use Docker to evaluate the project
    #[arg(long, overrides_with = "no_docker")]
    pub docker: bool,

    /// Do not use Docker
    #[arg(long = "no-docker", overrides_with = "docker")]
    pub no_docker: bool,

    /// Use a specific Docker client (such as depot, podman, etc.)
    #[arg(long)]
    pub docker_client: Option<String>,

    /// Override the install command
    #[arg(long)]
    pub install_command: Option<String>,

    /// Override the build command
    #[arg(long)]
    pub build_command: Option<String>,

    /// Override the start command
    #[arg(long)]
    pub start_command: Option<String>,

    /// Override detected provider
    #[arg(long)]
    pub provider: Option<String>,

    /// Port to use for serving (defaults to 8080)
    #[arg(long)]
    pub serve_port: Option<u16>,

    /// Output format
    #[arg(long, short = 'f', value_enum, default_value = "json")]
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
    /// Get the effective value of temp_shipit flag
    pub fn should_use_temp_shipit(&self) -> bool {
        self.temp_shipit && !self.no_temp_shipit
    }

    /// Get the effective value of regenerate flag
    pub fn should_regenerate(&self) -> bool {
        self.regenerate && !self.no_regenerate
    }

    /// Get the effective value of wasmer flag
    pub fn should_use_wasmer(&self) -> bool {
        self.wasmer && !self.no_wasmer
    }

    /// Get the effective value of docker flag
    pub fn should_use_docker(&self) -> bool {
        self.docker && !self.no_docker
    }

    /// Execute the plan command
    pub fn execute(&self, output: &Output) -> Result<()> {
        output.step("📋", "Loading plan...");
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
        let registry =
            crate::providers::ProviderRegistry::with_defaults();
        let provider_config = registry
            .detect_config(project_dir)
            .unwrap_or_default();

        // Evaluate Shipit file
        let (ctx, serve) = crate::starlark::evaluate_shipit_file(
            &shipit_path,
            provider_config,
        )
        .with_context(|| format!("Failed to evaluate {}", shipit_path.display()))?;

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
