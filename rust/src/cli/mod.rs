//! CLI interface and commands

pub mod args;
pub mod commands;
pub mod output;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Shipit CLI - Build and serve projects anywhere
#[derive(Parser, Debug)]
#[command(name = "shipit")]
#[command(version, about, long_about = None)]
#[command(arg_required_else_help = true)]
pub struct Cli {
    /// Enable verbose logging
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Suppress output
    #[arg(short, long, global = true, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Disable colored output
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Path to configuration file
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

/// Available CLI commands
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Detect, generate, build, and serve in one command
    Auto(commands::auto::AutoCommand),

    /// Generate a Shipit file for the project
    Generate(commands::generate::GenerateCommand),

    /// Show the build plan without executing
    Plan(commands::plan::PlanCommand),

    /// Build the project
    Build(commands::build::BuildCommand),

    /// Serve the built project
    Serve(commands::serve::ServeCommand),

    /// Deploy to Wasmer Edge
    Deploy(commands::deploy::DeployCommand),

    /// Manage configuration
    Config(commands::config::ConfigCommand),
}

impl Cli {
    /// Initialize logging based on verbosity flags
    pub fn init_logging(&self) {
        let level = if self.verbose {
            log::LevelFilter::Debug
        } else if self.quiet {
            log::LevelFilter::Off
        } else {
            log::LevelFilter::Info
        };

        env_logger::Builder::from_default_env()
            .filter_level(level)
            .format_timestamp(None)
            .format_module_path(false)
            .init();
    }

    /// Check if colors should be disabled
    pub fn should_disable_colors(&self) -> bool {
        self.no_color || !console::Term::stdout().features().is_attended()
    }
}
