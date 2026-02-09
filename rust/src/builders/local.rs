//! Local build backend that executes steps directly on the host.

use crate::builders::base::{copy_with_ignore, ensure_dir, extend_path, BuildBackend};
use crate::types::{Mount, Step};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;

/// Local build backend.
pub struct LocalBuildBackend {
    /// Source directory
    src_dir: PathBuf,
    /// Build directory (.shipit/local/build)
    build_path: PathBuf,
    /// Current working directory
    workdir: PathBuf,
    /// Runtime PATH after build
    runtime_path: Option<String>,
}

impl LocalBuildBackend {
    /// Create a new local build backend.
    pub fn new(src_dir: PathBuf, _assets_path: PathBuf) -> Self {
        let local_path = src_dir.join(".shipit").join("local");
        let build_path = local_path.join("build");
        let workdir = build_path.join("app");

        Self {
            src_dir,
            build_path: build_path.clone(),
            workdir,
            runtime_path: None,
        }
    }

    /// Get the path for a named mount.
    fn get_mount_path(&self, name: &str) -> PathBuf {
        if name == "app" {
            self.build_path.join("app")
        } else {
            self.build_path.join("opt").join(name)
        }
    }

    /// Execute a Run step.
    fn execute_run_step(
        &self,
        run: &str,
        inputs: &[String],
        env: &HashMap<String, String>,
    ) -> Result<()> {
        // Copy input files
        for input in inputs {
            let src = self.src_dir.join(input);
            let dest = self.workdir.join(input);
            if let Some(parent) = dest.parent() {
                ensure_dir(parent)?;
            }
            std::fs::copy(&src, &dest)
                .with_context(|| format!("Failed to copy input: {}", input))?;
        }

        if run.trim().is_empty() {
            anyhow::bail!("Empty command");
        }

        println!("🚀 Running: {}", run);

        // Execute command through shell to support:
        // - Shell builtins (echo, cd, etc.)
        // - Pipes and redirections
        // - Complex shell syntax
        let shell = if cfg!(target_os = "windows") {
            "cmd"
        } else {
            "sh"
        };

        let shell_flag = if cfg!(target_os = "windows") {
            "/C"
        } else {
            "-c"
        };

        let status = std::process::Command::new(shell)
            .arg(shell_flag)
            .arg(run)
            .current_dir(&self.workdir)
            .envs(env)
            .status()
            .with_context(|| format!("Failed to execute: {}", run))?;

        if !status.success() {
            anyhow::bail!("Command failed with exit code: {:?}", status.code());
        }

        Ok(())
    }

    /// Execute a Copy step.
    fn execute_copy_step(&self, source: &str, dest: &str) -> Result<()> {
        let dest_path = self.workdir.join(dest);

        if source.starts_with("http://") || source.starts_with("https://") {
            // Download from URL
            println!("⬇️  Downloading: {} -> {}", source, dest);

            let response = reqwest::blocking::get(source)
                .with_context(|| format!("Failed to download: {}", source))?;

            if !response.status().is_success() {
                anyhow::bail!("Download failed with status: {}", response.status());
            }

            if let Some(parent) = dest_path.parent() {
                ensure_dir(parent)?;
            }

            let bytes = response
                .bytes()
                .with_context(|| format!("Failed to read response: {}", source))?;

            std::fs::write(&dest_path, bytes)
                .with_context(|| format!("Failed to write file: {}", dest))?;
        } else {
            // Copy local file/directory
            println!("📋 Copying: {} -> {}", source, dest);

            let src_path = self.src_dir.join(source);

            if !src_path.exists() {
                anyhow::bail!("Source does not exist: {}", source);
            }

            if src_path.is_dir() {
                // Copy directory with ignore patterns
                let ignore_patterns = vec![
                    ".git".to_string(),
                    "node_modules".to_string(),
                    "__pycache__".to_string(),
                    ".shipit".to_string(),
                ];
                copy_with_ignore(&src_path, &dest_path, &ignore_patterns)?;
            } else {
                // Copy single file
                if let Some(parent) = dest_path.parent() {
                    ensure_dir(parent)?;
                }
                std::fs::copy(&src_path, &dest_path)
                    .with_context(|| format!("Failed to copy: {} -> {}", source, dest))?;
            }
        }

        Ok(())
    }
}

impl BuildBackend for LocalBuildBackend {
    fn get_build_mount_path(&self, name: &str) -> PathBuf {
        self.get_mount_path(name)
    }

    fn get_artifact_mount_path(&self, name: &str) -> PathBuf {
        // For local backend, build and artifact paths are the same
        self.get_mount_path(name)
    }

    fn execute_step(&mut self, step: &Step, env: &mut HashMap<String, String>) -> Result<()> {
        match step {
            Step::Use(use_step) => {
                println!("📦 Dependencies: {}", use_step.dependencies.join(", "));
                Ok(())
            }
            Step::Workdir(workdir_step) => {
                let new_dir = self.build_path.join(&workdir_step.path);
                ensure_dir(&new_dir)?;
                self.workdir = new_dir;
                println!("📂 Changed workdir to: {}", workdir_step.path.display());
                Ok(())
            }
            Step::Run(run_step) => {
                let inputs = run_step.inputs.as_deref().unwrap_or(&[]);
                self.execute_run_step(&run_step.command, inputs, env)
            }
            Step::Copy(copy_step) => self.execute_copy_step(&copy_step.source, &copy_step.target),
            Step::Env(env_step) => {
                for (key, value) in &env_step.variables {
                    env.insert(key.clone(), value.clone());
                    println!("🔧 Set {}={}", key, value);
                }
                Ok(())
            }
            Step::Path(path_step) => {
                let current = env.get("PATH").map(|s| s.as_str());
                let new_path = extend_path(current, &path_step.path);
                env.insert("PATH".to_string(), new_path);
                println!("🛤️  Extended PATH with: {}", path_step.path);
                Ok(())
            }
        }
    }

    fn build(
        &mut self,
        name: &str,
        mut env: HashMap<String, String>,
        mounts: &[Mount],
        steps: &[Step],
    ) -> Result<()> {
        println!("🔨 Building '{}' locally", name);

        // Clean and create build directory
        if self.build_path.exists() {
            std::fs::remove_dir_all(&self.build_path).context("Failed to clean build directory")?;
        }
        ensure_dir(&self.build_path)?;

        // Create mount directories
        for mount in mounts {
            let path = self.get_mount_path(&mount.name);
            ensure_dir(&path)?;
            println!("📁 Created mount: {}", mount.name);
        }

        // Execute steps
        for step in steps {
            self.execute_step(step, &mut env)?;
        }

        // Save runtime PATH
        self.runtime_path = env.get("PATH").cloned();

        println!("✅ Build completed successfully");
        Ok(())
    }

    fn get_runtime_path(&self) -> Option<String> {
        self.runtime_path.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let backend =
            LocalBuildBackend::new(PathBuf::from("/test/src"), PathBuf::from("/test/assets"));

        assert_eq!(backend.src_dir, PathBuf::from("/test/src"));
        assert_eq!(
            backend.build_path,
            PathBuf::from("/test/src/.shipit/local/build")
        );
        assert_eq!(
            backend.workdir,
            PathBuf::from("/test/src/.shipit/local/build/app")
        );
    }

    #[test]
    fn test_get_mount_path_app() {
        let backend =
            LocalBuildBackend::new(PathBuf::from("/test/src"), PathBuf::from("/test/assets"));

        assert_eq!(
            backend.get_mount_path("app"),
            PathBuf::from("/test/src/.shipit/local/build/app")
        );
    }

    #[test]
    fn test_get_mount_path_other() {
        let backend =
            LocalBuildBackend::new(PathBuf::from("/test/src"), PathBuf::from("/test/assets"));

        assert_eq!(
            backend.get_mount_path("temp"),
            PathBuf::from("/test/src/.shipit/local/build/opt/temp")
        );
    }
}
