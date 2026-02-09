//! Config command - manage configuration

use crate::cli::output::Output;
use anyhow::{Context, Result};
use clap::{Args, Subcommand};

/// Manage configuration
#[derive(Args, Debug)]
pub struct ConfigCommand {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Show current configuration
    Show,

    /// Set a configuration value
    Set {
        /// Configuration key
        key: String,
        /// Configuration value
        value: String,
    },

    /// Get configuration path
    Path,

    /// Reset to default configuration
    Reset,
}

impl ConfigCommand {
    /// Execute the config command
    pub fn execute(&self, output: &Output) -> Result<()> {
        match &self.action {
            ConfigAction::Show => self.show_config(output),
            ConfigAction::Set { key, value } => self.set_config(key, value, output),
            ConfigAction::Path => self.show_path(output),
            ConfigAction::Reset => self.reset_config(output),
        }
    }

    fn show_config(&self, output: &Output) -> Result<()> {
        output.step("⚙️", "Current configuration");
        output.blank();

        let config = crate::config::Config::load_layered(None)?;

        let toml = toml::to_string_pretty(&config)?;
        println!("{}", toml);

        Ok(())
    }

    fn set_config(&self, _key: &str, _value: &str, output: &Output) -> Result<()> {
        output.error("Setting config values is not yet implemented");
        output.info("Configuration management will be enhanced in a future version");
        anyhow::bail!("Not implemented");
    }

    fn show_path(&self, output: &Output) -> Result<()> {
        output.info("Config paths searched (in order):");
        output.info("  1. SHIPIT_CONFIG environment variable");
        output.info("  2. ./shipit.toml (project level)");
        output.info("  3. ~/.config/shipit/config.toml (user level)");
        output.info("  4. /etc/shipit/config.toml (system level)");
        output.blank();
        output.info("Current config loaded from layered sources");

        Ok(())
    }

    fn reset_config(&self, output: &Output) -> Result<()> {
        output.info("Reset will clear user-level configuration");
        output.info("User config path: ~/.config/shipit/config.toml");
        output.blank();

        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(std::path::PathBuf::from)
            .or_else(|_| std::env::current_dir())
            .context("Could not determine home directory")?;

        let config_path = home.join(".config").join("shipit").join("config.toml");

        if !config_path.exists() {
            output.info("No user config file exists");
            return Ok(());
        }

        output.step("⚙️", "Resetting configuration");
        std::fs::remove_file(&config_path)?;
        output.success("User configuration reset to defaults");

        Ok(())
    }
}
