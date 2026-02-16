//! Wasmer runner implementation.

use crate::builders::base::BuildBackend;
use crate::runners::base::{generate_bash_script, make_executable, Runner};
use crate::runners::wasmer::manifest::generate_manifest;
use crate::types::serve::{PrepareStep, Serve};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Wasmer runner.
pub struct WasmerRunner {
    build_backend: Arc<dyn BuildBackend>,
    src_dir: PathBuf,
    wasmer_dir_path: PathBuf,
    wasmer_registry: Option<String>,
    wasmer_token: Option<String>,
    bin: String,
}

impl WasmerRunner {
    /// Create a new Wasmer runner.
    pub fn new(
        build_backend: Arc<dyn BuildBackend>,
        src_dir: PathBuf,
        registry: Option<String>,
        token: Option<String>,
        bin: Option<String>,
    ) -> Self {
        let wasmer_dir_path = src_dir.join(".shipit").join("wasmer");
        let bin = bin.unwrap_or_else(|| "wasmer".to_string());

        Self {
            build_backend,
            src_dir,
            wasmer_dir_path,
            wasmer_registry: registry,
            wasmer_token: token,
            bin,
        }
    }

    /// Build prepare script.
    fn build_prepare(&mut self, serve: &Serve) -> Result<()> {
        let prepare = match &serve.prepare {
            Some(p) => p,
            None => return Ok(()),
        };

        println!("📝 Generating Wasmer prepare script...");

        // Create prepare directory
        let prepare_dir = self.wasmer_dir_path.join("prepare");
        std::fs::create_dir_all(&prepare_dir).context("Failed to create prepare directory")?;

        // Build commands
        let commands: Vec<String> = prepare.iter().map(|p| p.command.clone()).collect();

        // Generate script
        let cwd = serve.cwd.as_ref().map(PathBuf::from);
        let script = generate_bash_script(&commands, cwd.as_deref());

        // Write script
        let script_path = prepare_dir.join("prepare.sh");
        std::fs::write(&script_path, &script).context("Failed to write prepare script")?;

        // Make executable
        make_executable(&script_path)?;

        println!("\n📄 Prepare script:");
        println!("{}", "=".repeat(60));
        for line in script.lines() {
            println!("{}", line);
        }
        println!("{}", "=".repeat(60));
        println!();

        Ok(())
    }

    /// Build wasmer.toml manifest.
    fn build_serve(&mut self, serve: &Serve) -> Result<()> {
        println!("📝 Generating wasmer.toml manifest...");

        // Generate manifest
        let manifest = generate_manifest(serve, self.build_backend.as_ref())?;

        // Write manifest
        let manifest_path = self.wasmer_dir_path.join("wasmer.toml");
        std::fs::write(&manifest_path, &manifest).context("Failed to write wasmer.toml")?;

        // Print manifest
        println!("\n📄 Wasmer manifest:");
        println!("{}", "=".repeat(60));
        for line in manifest.lines() {
            println!("{}", line);
        }
        println!("{}", "=".repeat(60));
        println!();

        Ok(())
    }

    /// Deploy to Wasmer Edge.
    pub fn deploy(&self, app_name: &str) -> Result<()> {
        let registry = self
            .wasmer_registry
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Wasmer registry not configured"))?;
        let token = self
            .wasmer_token
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Wasmer token not configured"))?;

        println!("🚀 Deploying to Wasmer Edge: {}", app_name);

        let manifest_path = self.wasmer_dir_path.join("wasmer.toml");
        if !manifest_path.exists() {
            anyhow::bail!("wasmer.toml not found. Run build first.");
        }

        let status = std::process::Command::new(&self.bin)
            .arg("deploy")
            .arg("--registry")
            .arg(registry)
            .arg("--token")
            .arg(token)
            .arg("--app-name")
            .arg(app_name)
            .arg("--manifest")
            .arg(&manifest_path)
            .current_dir(&self.src_dir)
            .status()
            .context("Failed to execute wasmer deploy")?;

        if !status.success() {
            anyhow::bail!("Wasmer deploy failed");
        }

        println!("✅ Deployment completed");
        Ok(())
    }
}

impl Runner for WasmerRunner {
    fn get_serve_mount_path(&self, name: &str) -> PathBuf {
        if name == "app" {
            PathBuf::from("/app")
        } else {
            PathBuf::from("/opt").join(name)
        }
    }

    fn build(&mut self, serve: &Serve) -> Result<()> {
        println!("🏗️  Building Wasmer runner");

        // Clean and create directory
        if self.wasmer_dir_path.exists() {
            std::fs::remove_dir_all(&self.wasmer_dir_path)
                .context("Failed to clean wasmer directory")?;
        }
        std::fs::create_dir_all(&self.wasmer_dir_path)
            .context("Failed to create wasmer directory")?;

        self.build_prepare(serve)?;
        self.build_serve(serve)?;

        println!("✅ Wasmer runner build completed");
        Ok(())
    }

    fn prepare(&self, env: &HashMap<String, String>, _prepare: &[PrepareStep]) -> Result<()> {
        let prepare_script = self.wasmer_dir_path.join("prepare").join("prepare.sh");
        if !prepare_script.exists() {
            return Ok(());
        }

        println!("🔧 Running prepare with Wasmer...");

        let prepare_dir = self.wasmer_dir_path.join("prepare");

        let mut cmd = std::process::Command::new(&self.bin);
        cmd.arg("run")
            .arg("bash")
            .arg("--volume")
            .arg(format!("{}:{}", prepare_dir.display(), "/prepare"))
            .arg("--")
            .arg("/prepare/prepare.sh")
            .current_dir(&self.src_dir);

        // Add environment variables
        for (key, value) in env {
            cmd.env(key, value);
        }

        let status = cmd.status().context("Failed to execute wasmer prepare")?;

        if !status.success() {
            anyhow::bail!("Prepare failed");
        }

        println!("✅ Prepare completed");
        Ok(())
    }

    fn run_serve_command(&self, command: &str) -> Result<()> {
        let manifest_path = self.wasmer_dir_path.join("wasmer.toml");
        if !manifest_path.exists() {
            anyhow::bail!("wasmer.toml not found. Run build first.");
        }

        if command != "start" {
            anyhow::bail!(
                "Wasmer runner currently supports only the 'start' command, got '{}'",
                command
            );
        }

        println!("🚀 Running command with Wasmer: {}", command);

        let status = std::process::Command::new(&self.bin)
            .arg("run")
            .arg("--net")
            .arg(&self.wasmer_dir_path)
            .current_dir(&self.src_dir)
            .status()
            .with_context(|| format!("Failed to execute wasmer run: {}", command))?;

        if !status.success() {
            anyhow::bail!("Command '{}' failed", command);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builders::local::LocalBuildBackend;

    #[test]
    fn test_new() {
        let backend = Arc::new(LocalBuildBackend::new(
            PathBuf::from("/test/src"),
            PathBuf::from("/test/assets"),
        ));
        let runner = WasmerRunner::new(backend, PathBuf::from("/test/src"), None, None, None);

        assert_eq!(
            runner.wasmer_dir_path,
            PathBuf::from("/test/src/.shipit/wasmer")
        );
        assert_eq!(runner.bin, "wasmer");
    }

    #[test]
    fn test_new_custom_bin() {
        let backend = Arc::new(LocalBuildBackend::new(
            PathBuf::from("/test/src"),
            PathBuf::from("/test/assets"),
        ));
        let runner = WasmerRunner::new(
            backend,
            PathBuf::from("/test/src"),
            None,
            None,
            Some("custom-wasmer".to_string()),
        );

        assert_eq!(runner.bin, "custom-wasmer");
    }

    #[test]
    fn test_get_serve_mount_path() {
        let backend = Arc::new(LocalBuildBackend::new(
            PathBuf::from("/test/src"),
            PathBuf::from("/test/assets"),
        ));
        let runner = WasmerRunner::new(backend, PathBuf::from("/test/src"), None, None, None);

        assert_eq!(runner.get_serve_mount_path("app"), PathBuf::from("/app"));
        assert_eq!(
            runner.get_serve_mount_path("temp"),
            PathBuf::from("/opt/temp")
        );
    }

    #[test]
    fn test_deploy_no_registry() {
        let backend = Arc::new(LocalBuildBackend::new(
            PathBuf::from("/test/src"),
            PathBuf::from("/test/assets"),
        ));
        let runner = WasmerRunner::new(backend, PathBuf::from("/test/src"), None, None, None);

        let result = runner.deploy("test-app");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("registry"));
    }
}
