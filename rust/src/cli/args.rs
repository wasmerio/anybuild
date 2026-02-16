//! Shared CLI argument groups

use clap::Args;
use std::path::PathBuf;

/// Arguments for build configuration
#[derive(Args, Debug, Clone)]
pub struct BuildArgs {
    /// Use local build backend (default)
    #[arg(long, conflicts_with = "docker")]
    pub local: bool,

    /// Use Docker build backend
    #[arg(long, conflicts_with = "local")]
    pub docker: bool,

    /// Specify build backend by name
    #[arg(long, conflicts_with_all = ["local", "docker"])]
    pub backend: Option<String>,

    /// Clean build directory before building
    #[arg(long)]
    pub clean: bool,

    /// Force rebuild even if cached
    #[arg(long)]
    pub rebuild: bool,
}

impl BuildArgs {
    /// Get the selected backend name
    pub fn backend_name(&self) -> &str {
        if let Some(ref name) = self.backend {
            name
        } else if self.docker {
            "docker"
        } else {
            "local"
        }
    }

    /// Check if using Docker backend
    pub fn is_docker(&self) -> bool {
        self.docker || self.backend.as_deref() == Some("docker")
    }

    /// Check if using local backend
    pub fn is_local(&self) -> bool {
        self.local || (!self.docker && self.backend.is_none())
    }
}

/// Arguments for serve configuration
#[derive(Args, Debug, Clone)]
pub struct ServeArgs {
    /// Use Wasmer runner
    #[arg(long)]
    pub wasmer: bool,

    /// Run the start command
    #[arg(long, conflicts_with = "command")]
    pub start: bool,

    /// Specify command to run (e.g., web, worker)
    #[arg(long)]
    pub command: Option<String>,

    /// Override port number
    #[arg(long, short)]
    pub port: Option<u16>,

    /// Pass environment variables (KEY=VALUE)
    #[arg(long, short)]
    pub env: Vec<String>,

    /// Run prepare steps before serving
    #[arg(long)]
    pub prepare: bool,
}

impl ServeArgs {
    /// Check if using Wasmer runner
    pub fn is_wasmer(&self) -> bool {
        self.wasmer
    }

    /// Parse environment variables into a HashMap
    pub fn parse_env_vars(&self) -> anyhow::Result<std::collections::HashMap<String, String>> {
        let mut env = std::collections::HashMap::new();

        for var in &self.env {
            let parts: Vec<&str> = var.splitn(2, '=').collect();
            if parts.len() != 2 {
                anyhow::bail!(
                    "Invalid environment variable format: {} (expected KEY=VALUE)",
                    var
                );
            }
            env.insert(parts[0].to_string(), parts[1].to_string());
        }

        Ok(env)
    }
}

/// Arguments for deployment configuration
#[derive(Args, Debug, Clone)]
pub struct DeployArgs {
    /// Wasmer registry namespace
    #[arg(long, short)]
    pub registry: Option<String>,

    /// Authentication token (or set WASMER_TOKEN env var)
    #[arg(long, short, env = "WASMER_TOKEN")]
    pub token: Option<String>,

    /// Application name
    #[arg(long, short)]
    pub app: String,

    /// Make deployment public
    #[arg(long)]
    pub public: bool,
}

impl DeployArgs {
    /// Validate deployment arguments
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.registry.is_none() {
            anyhow::bail!("Registry namespace is required (use --registry or set WASMER_REGISTRY)");
        }
        if self.token.is_none() {
            anyhow::bail!("Authentication token is required (use --token or set WASMER_TOKEN)");
        }
        Ok(())
    }
}

/// Common path argument
#[derive(Args, Debug, Clone)]
pub struct PathArg {
    /// Path to project directory or Shipit file
    #[arg(default_value = ".")]
    pub path: PathBuf,
}

impl PathArg {
    /// Get the path as a PathBuf
    pub fn as_path(&self) -> &PathBuf {
        &self.path
    }

    /// Check if path exists
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Resolve to absolute path
    pub fn canonicalize(&self) -> anyhow::Result<PathBuf> {
        self.path
            .canonicalize()
            .map_err(|e| anyhow::anyhow!("Failed to resolve path {:?}: {}", self.path, e))
    }
}
