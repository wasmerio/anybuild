//! Generate command - create Shipit file

use crate::cli::output::Output;
use crate::providers::registry::ProviderRegistry;
use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

/// Generate a Shipit file for the project
#[derive(Args, Debug)]
#[command(after_help = "EXAMPLES:\n  \
    shipit generate                    # Generate Shipit for current directory\n  \
    shipit generate my-app             # Generate Shipit for 'my-app' directory\n  \
    shipit generate -o custom.Shipit   # Output to custom file\n  \
    shipit generate --provider nodejs  # Force specific provider\n  \
    shipit generate --dry-run          # Preview without writing")]
pub struct GenerateCommand {
    /// Path to project directory
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Override detected provider
    #[arg(long, short)]
    pub provider: Option<String>,

    /// Force overwrite existing Shipit file
    #[arg(long, short)]
    pub force: bool,

    /// Output path for Shipit file
    #[arg(long, short, visible_alias = "shipit-path", alias = "out")]
    pub output: Option<PathBuf>,

    /// JSON configuration content to override provider config
    #[arg(long)]
    pub config: Option<String>,

    /// Override the install command
    #[arg(long)]
    pub install_command: Option<String>,

    /// Override the build command
    #[arg(long)]
    pub build_command: Option<String>,

    /// Override the start command
    #[arg(long)]
    pub start_command: Option<String>,

    /// Show generated file without writing
    #[arg(long)]
    pub dry_run: bool,
}

impl GenerateCommand {
    /// Execute the generate command
    pub fn execute(&self, output: &Output) -> Result<()> {
        output.step("🔍", "Detecting project type...");

        // Load configuration
        let config = crate::config::Config::load_layered(None).unwrap_or_default();

        // Setup provider registry
        let registry = ProviderRegistry::with_defaults();

        // Detect or load specified provider
        let provider = if let Some(ref name) = self.provider {
            output.info(format!("Using specified provider: {}", name));
            crate::generator::load_provider(&self.path, &registry, &config, Some(name))?
        } else {
            let pb = output.progress("Scanning project...");
            let provider = crate::generator::detect_provider(&self.path, &registry, &config)?;
            pb.finish_and_clear();
            output.success(format!("Detected: {}", provider.name()));
            provider
        };

        output.blank();
        output.step("📝", "Generating Shipit file...");

        // Determine output path
        let shipit_path = self
            .output
            .clone()
            .unwrap_or_else(|| self.path.join("Shipit"));

        // Check if file exists
        if shipit_path.exists() && !self.force && !self.dry_run {
            anyhow::bail!(
                "Shipit file already exists at {}. Use --force to overwrite",
                shipit_path.display()
            );
        }

        // Get provider plan
        let plan = provider
            .plan(&self.path)
            .context("Failed to generate provider plan")?;

        // Generate the file content
        let content = crate::generator::generate_shipit_file(&self.path, &plan)
            .context("Failed to generate Shipit content")?;

        if self.dry_run {
            // Preview mode - display only
            output.blank();
            output.header("Generated Shipit file");
            println!("{}", content);
            output.blank();
            output.info(format!("Would write to: {}", shipit_path.display()));
        } else {
            // Actually write the file
            std::fs::write(&shipit_path, &content).context("Failed to write Shipit file")?;

            output.success(format!("Generated: {}", shipit_path.display()));

            // Show a preview of the generated file
            let lines: Vec<&str> = content.lines().take(10).collect();
            output.blank();
            output.info("Preview (first 10 lines):");
            for line in lines {
                println!("  {}", line);
            }
            if content.lines().count() > 10 {
                println!("  ...");
            }
        }

        Ok(())
    }
}
