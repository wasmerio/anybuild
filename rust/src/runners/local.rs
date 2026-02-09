//! Local runner that executes commands directly on the host.

use crate::builders::base::BuildBackend;
use crate::runners::base::{format_env_vars, generate_bash_script, make_executable, Runner};
use crate::types::serve::{PrepareStep, Serve};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Local runner.
pub struct LocalRunner {
    build_backend: Arc<dyn BuildBackend>,
    src_dir: PathBuf,
    serve_bin_path: PathBuf,
    prepare_bash_script: PathBuf,
}

impl LocalRunner {
    /// Create a new local runner.
    pub fn new(build_backend: Arc<dyn BuildBackend>, src_dir: PathBuf) -> Self {
        let runner_path = src_dir.join(".shipit").join("runner").join("local");
        let serve_bin_path = runner_path.join("serve").join("bin");
        let prepare_bash_script = runner_path.join("prepare").join("prepare.sh");

        Self {
            build_backend,
            src_dir,
            serve_bin_path,
            prepare_bash_script,
        }
    }

    /// Build prepare script.
    fn build_prepare(&mut self, serve: &Serve) -> Result<()> {
        let prepare = match &serve.prepare {
            Some(p) => p,
            None => return Ok(()),
        };

        println!("📝 Generating prepare script...");

        // Create prepare directory
        let prepare_dir = self.prepare_bash_script.parent().unwrap();
        std::fs::create_dir_all(prepare_dir).context("Failed to create prepare directory")?;

        // Build commands
        let commands: Vec<String> = prepare.iter().map(|p| p.command.clone()).collect();

        // Generate script
        let cwd = serve.cwd.as_ref().map(PathBuf::from);
        let script = generate_bash_script(&commands, cwd.as_deref());

        // Write script
        std::fs::write(&self.prepare_bash_script, &script)
            .context("Failed to write prepare script")?;

        // Make executable
        make_executable(&self.prepare_bash_script)?;

        // Print script
        println!(
            "\n📄 Prepare script ({}):",
            self.prepare_bash_script.display()
        );
        println!("{}", "=".repeat(60));
        for line in script.lines() {
            println!("{}", line);
        }
        println!("{}", "=".repeat(60));
        println!();

        Ok(())
    }

    /// Build serve scripts.
    fn build_serve(&mut self, serve: &Serve) -> Result<()> {
        println!("📝 Generating serve scripts...");

        // Clean and create serve bin directory
        if self.serve_bin_path.exists() {
            std::fs::remove_dir_all(&self.serve_bin_path)
                .context("Failed to clean serve bin directory")?;
        }
        std::fs::create_dir_all(&self.serve_bin_path)
            .context("Failed to create serve bin directory")?;

        // Get runtime PATH
        let runtime_path = self.build_backend.get_runtime_path();

        // Generate script for each command
        for (name, command_str) in &serve.commands {
            let script_path = self.serve_bin_path.join(name);

            // Build script lines
            let mut lines = vec![
                "#!/bin/bash".to_string(),
                "set -e".to_string(),
                "".to_string(),
            ];

            // Add cd if needed
            if let Some(cwd) = &serve.cwd {
                lines.push(format!("cd {}", cwd));
                lines.push("".to_string());
            }

            // Add PATH prefix if runtime path exists
            if let Some(path) = &runtime_path {
                lines.push(format!("export PATH=\"{}\"", path));
            }

            // Add environment variables
            if let Some(env) = &serve.env {
                lines.extend(format_env_vars(env));
            }

            if !lines.is_empty() && !lines.last().unwrap().is_empty() {
                lines.push("".to_string());
            }

            // Add command
            lines.push(command_str.clone());

            let script = lines.join("\n");

            // Write script
            std::fs::write(&script_path, &script)
                .with_context(|| format!("Failed to write serve script: {}", name))?;

            // Make executable
            make_executable(&script_path)?;

            // Print script
            println!("\n📄 Serve script '{}':", name);
            println!("{}", "=".repeat(60));
            for line in script.lines() {
                println!("{}", line);
            }
            println!("{}", "=".repeat(60));
            println!();
        }

        Ok(())
    }
}

impl Runner for LocalRunner {
    fn get_serve_mount_path(&self, name: &str) -> PathBuf {
        self.build_backend.get_artifact_mount_path(name)
    }

    fn build(&mut self, serve: &Serve) -> Result<()> {
        println!("🏗️  Building local runner");

        self.build_prepare(serve)?;
        self.build_serve(serve)?;

        println!("✅ Local runner build completed");
        Ok(())
    }

    fn prepare(&self, env: &HashMap<String, String>, _prepare: &[PrepareStep]) -> Result<()> {
        if !self.prepare_bash_script.exists() {
            return Ok(());
        }

        println!("🔧 Running prepare script...");

        let status = std::process::Command::new(&self.prepare_bash_script)
            .current_dir(&self.src_dir)
            .envs(env)
            .status()
            .context("Failed to execute prepare script")?;

        if !status.success() {
            anyhow::bail!("Prepare script failed");
        }

        println!("✅ Prepare completed");
        Ok(())
    }

    fn run_serve_command(&self, command: &str) -> Result<()> {
        let script_path = self.serve_bin_path.join(command);

        if !script_path.exists() {
            anyhow::bail!("Serve command '{}' not found", command);
        }

        println!("🚀 Running serve command: {}", command);

        let status = std::process::Command::new(&script_path)
            .current_dir(&self.src_dir)
            .status()
            .with_context(|| format!("Failed to execute serve command: {}", command))?;

        if !status.success() {
            anyhow::bail!("Serve command '{}' failed", command);
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
        let runner = LocalRunner::new(backend, PathBuf::from("/test/src"));

        assert_eq!(
            runner.serve_bin_path,
            PathBuf::from("/test/src/.shipit/runner/local/serve/bin")
        );
    }

    #[test]
    fn test_get_serve_mount_path() {
        let backend = Arc::new(LocalBuildBackend::new(
            PathBuf::from("/test/src"),
            PathBuf::from("/test/assets"),
        ));
        let runner = LocalRunner::new(backend, PathBuf::from("/test/src"));

        let path = runner.get_serve_mount_path("app");
        assert!(path.to_string_lossy().contains("app"));
    }
}
