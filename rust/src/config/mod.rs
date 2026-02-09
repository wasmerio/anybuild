//! Configuration system for Shipit

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

pub mod commands;
pub mod env;

pub use commands::CustomCommands;
pub use env::{expand_env_vars, load_env};

/// Main configuration struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Port for the application (default: 8080)
    #[serde(default = "default_port")]
    pub port: u16,
    /// Custom commands
    #[serde(default)]
    pub commands: CustomCommands,
    /// Additional environment variables
    #[serde(default)]
    pub env_vars: HashMap<String, String>,
}

fn default_port() -> u16 {
    8080
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: default_port(),
            commands: CustomCommands::default(),
            env_vars: HashMap::new(),
        }
    }
}

impl Config {
    /// Create a new empty config
    pub fn new() -> Self {
        Self::default()
    }

    /// Load configuration from a file (supports YAML, TOML, JSON)
    pub fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        // Determine format by extension
        let config = match path.extension().and_then(|s| s.to_str()) {
            Some("yaml") | Some("yml") => {
                serde_yaml::from_str(&contents).context("Failed to parse YAML config")?
            }
            Some("toml") => toml::from_str(&contents).context("Failed to parse TOML config")?,
            Some("json") => {
                serde_json::from_str(&contents).context("Failed to parse JSON config")?
            }
            _ => {
                anyhow::bail!("Unsupported config file format. Use .yaml, .toml, or .json");
            }
        };

        Ok(config)
    }

    /// Load configuration using figment with layering
    pub fn load_layered(path: Option<&Path>) -> Result<Self> {
        use figment::{
            providers::{Env, Format, Json, Toml, Yaml},
            Figment,
        };

        let mut figment = Figment::new();

        // Layer 1: Default values (already in the struct)

        // Layer 2: File-based config if provided
        if let Some(p) = path {
            if p.exists() {
                figment = match p.extension().and_then(|s| s.to_str()) {
                    Some("yaml") | Some("yml") => figment.merge(Yaml::file(p)),
                    Some("toml") => figment.merge(Toml::file(p)),
                    Some("json") => figment.merge(Json::file(p)),
                    _ => figment,
                };
            }
        }

        // Layer 3: Environment variables (prefixed with SHIPIT_)
        figment = figment.merge(Env::prefixed("SHIPIT_").split("_"));

        let config: Config = figment
            .extract()
            .context("Failed to extract configuration")?;

        Ok(config)
    }

    /// Set port
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Set commands
    pub fn with_commands(mut self, commands: CustomCommands) -> Self {
        self.commands = commands;
        self
    }

    /// Add environment variable
    pub fn add_env_var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.insert(key.into(), value.into());
        self
    }

    /// Merge with another config, preferring values from other
    pub fn merge(mut self, other: Config) -> Self {
        if other.port != default_port() {
            self.port = other.port;
        }
        self.commands = self.commands.merge(other.commands);
        self.env_vars.extend(other.env_vars);
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        self.commands.validate()?;

        if self.port == 0 {
            anyhow::bail!("Port cannot be 0");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.port, 8080);
        assert!(config.commands.is_empty());
        assert!(config.env_vars.is_empty());
    }

    #[test]
    fn test_config_builder() {
        let config = Config::new()
            .with_port(3000)
            .add_env_var("NODE_ENV", "production");

        assert_eq!(config.port, 3000);
        assert_eq!(
            config.env_vars.get("NODE_ENV"),
            Some(&"production".to_string())
        );
    }

    #[test]
    fn test_load_yaml() {
        let yaml = r#"
port: 3000
commands:
  build: "npm run build"
  start: "npm start"
env_vars:
  NODE_ENV: "production"
"#;

        let file = NamedTempFile::new().unwrap();
        let path = file.path().with_extension("yaml");
        std::fs::write(&path, yaml).unwrap();

        let config = Config::load_from_file(&path).unwrap();
        assert_eq!(config.port, 3000);
        assert_eq!(config.commands.build, Some("npm run build".to_string()));
        assert_eq!(
            config.env_vars.get("NODE_ENV"),
            Some(&"production".to_string())
        );

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_load_toml() {
        let toml = r#"
port = 4000

[commands]
build = "cargo build"
start = "cargo run"

[env_vars]
RUST_LOG = "debug"
"#;

        let file = NamedTempFile::new().unwrap();
        let path = file.path().with_extension("toml");
        std::fs::write(&path, toml).unwrap();

        let config = Config::load_from_file(&path).unwrap();
        assert_eq!(config.port, 4000);
        assert_eq!(config.commands.build, Some("cargo build".to_string()));

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_load_json() {
        let json = r#"{
  "port": 5000,
  "commands": {
    "start": "python app.py"
  }
}"#;

        let file = NamedTempFile::new().unwrap();
        let path = file.path().with_extension("json");
        std::fs::write(&path, json).unwrap();

        let config = Config::load_from_file(&path).unwrap();
        assert_eq!(config.port, 5000);
        assert_eq!(config.commands.start, Some("python app.py".to_string()));

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_config_merge() {
        let config1 = Config::new().with_port(3000).add_env_var("VAR1", "value1");

        let config2 = Config::new().with_port(4000).add_env_var("VAR2", "value2");

        let merged = config1.merge(config2);
        assert_eq!(merged.port, 4000);
        assert_eq!(merged.env_vars.len(), 2);
    }

    #[test]
    fn test_validation() {
        let config = Config::new();
        assert!(config.validate().is_ok());

        let config = Config::new().with_port(0);
        assert!(config.validate().is_err());
    }
}
