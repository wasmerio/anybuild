//! Docker build backend that executes steps in containers.

use crate::builders::base::BuildBackend;
use crate::types::{Mount, Step};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;

/// Mise package configuration
#[derive(Debug, Clone)]
struct MiseConfig {
    source: String,
    postinstall: Option<String>,
}

/// Docker build backend.
pub struct DockerBuildBackend {
    /// Source directory
    src_dir: PathBuf,
    /// Assets directory
    #[allow(dead_code)]
    assets_path: PathBuf,
    /// Docker directory (.shipit/docker)
    docker_path: PathBuf,
    /// Docker output directory (.shipit/docker/out)
    docker_out_path: PathBuf,
    /// Dockerfile path
    docker_file_path: PathBuf,
    /// Docker image name file
    docker_name_path: PathBuf,
    /// Dockerignore file path
    docker_ignore_path: PathBuf,
    /// Docker client command
    docker_client: String,
    /// Extra docker options
    docker_opts: Option<String>,
    /// Runtime PATH after build
    runtime_path: Option<String>,
}

impl DockerBuildBackend {
    /// Create a new docker build backend.
    pub fn new(
        src_dir: PathBuf,
        assets_path: PathBuf,
        docker_client: Option<String>,
        docker_opts: Option<String>,
    ) -> Self {
        let docker_path = src_dir.join(".shipit").join("docker");
        let docker_out_path = docker_path.join("out");
        let docker_file_path = docker_path.join("Dockerfile");
        let docker_name_path = docker_path.join("name");
        let docker_ignore_path = docker_path.join(".dockerignore");

        Self {
            src_dir,
            assets_path,
            docker_path,
            docker_out_path,
            docker_file_path,
            docker_name_path,
            docker_ignore_path,
            docker_client: docker_client.unwrap_or_else(|| "docker".to_string()),
            docker_opts,
            runtime_path: None,
        }
    }

    /// Get Mise configuration for a package.
    fn get_mise_config(package_name: &str) -> Option<MiseConfig> {
        let mapper: HashMap<&str, MiseConfig> = [
            (
                "php",
                MiseConfig {
                    source: "ubi:adwinying/php".to_string(),
                    postinstall: None,
                },
            ),
            (
                "go-wasix",
                MiseConfig {
                    source: "ubi:wasix-org/go[extract_all=true,bin_path=bin/]".to_string(),
                    postinstall: None,
                },
            ),
            (
                "composer",
                MiseConfig {
                    source: "ubi:composer/composer".to_string(),
                    postinstall: Some("ln -sf ~/.local/share/mise/installs/composer/latest/composer /mise/shims/composer".to_string()),
                },
            ),
            (
                "hugo",
                MiseConfig {
                    source: "ubi:gohugoio/hugo[matching=extended]".to_string(),
                    postinstall: None,
                },
            ),
        ]
        .iter()
        .cloned()
        .collect();

        mapper.get(package_name).cloned()
    }

    /// Get the path for a named mount (relative).
    fn get_mount_path(&self, name: &str) -> PathBuf {
        if name == "app" {
            PathBuf::from("app")
        } else {
            PathBuf::from("opt").join(name)
        }
    }

    /// Generate Dockerfile header with base image and mise setup.
    fn generate_dockerfile_header(&self) -> Vec<String> {
        vec![
            "# syntax=docker/dockerfile:1.7-labs".to_string(),
            "".to_string(),
            "FROM debian:trixie-slim AS build".to_string(),
            "".to_string(),
            "# Install system dependencies".to_string(),
            "RUN apt-get update && apt-get install -y \\".to_string(),
            "    build-essential gcc make autoconf libtool \\".to_string(),
            "    libmariadb-dev libpq-dev libvips-dev \\".to_string(),
            "    curl ca-certificates unzip git \\".to_string(),
            "    && rm -rf /var/lib/apt/lists/*".to_string(),
            "".to_string(),
            "# Set up Mise environment".to_string(),
            "ENV MISE_DATA_DIR=/mise \\".to_string(),
            "    MISE_CONFIG_DIR=/mise \\".to_string(),
            "    MISE_CACHE_DIR=/mise/cache \\".to_string(),
            "    PATH=/mise/shims:$PATH".to_string(),
            "".to_string(),
            "# Install mise".to_string(),
            "RUN curl -fsSL https://mise.jdx.dev/install.sh | sh && \\".to_string(),
            "    mv ~/.local/bin/mise /usr/local/bin/mise && \\".to_string(),
            "    mise --version".to_string(),
            "".to_string(),
        ]
    }

    /// Convert a step to Dockerfile lines.
    fn step_to_dockerfile(&self, step: &Step, env: &mut HashMap<String, String>) -> Vec<String> {
        match step {
            Step::Use(use_step) => {
                let mut lines = vec![];
                for dep in &use_step.dependencies {
                    // Parse dependency: ref:package:name or ref:package:name@version
                    let parts: Vec<&str> = dep.split(':').collect();
                    if parts.len() < 3 {
                        continue;
                    }

                    let package_name = parts[2];
                    let (name, version) = if let Some(pos) = package_name.find('@') {
                        (&package_name[..pos], Some(&package_name[pos + 1..]))
                    } else {
                        (package_name, None)
                    };

                    if let Some(mise_cfg) = Self::get_mise_config(name) {
                        lines.push(format!("RUN mise use -g {}", mise_cfg.source));
                        if let Some(postinstall) = mise_cfg.postinstall {
                            lines.push(format!("RUN {}", postinstall));
                        }
                    } else if let Some(ver) = version {
                        lines.push(format!("RUN mise use -g {}@{}", name, ver));
                    } else {
                        lines.push(format!("RUN mise use -g {}", name));
                    }
                }
                if !lines.is_empty() {
                    lines.push("".to_string());
                }
                lines
            }
            Step::Workdir(workdir_step) => {
                vec![
                    format!("WORKDIR /{}", workdir_step.path.display()),
                    "".to_string(),
                ]
            }
            Step::Run(run_step) => {
                let mut lines = vec![];

                // Copy inputs if needed
                if let Some(inputs) = &run_step.inputs {
                    for input in inputs {
                        lines.push(format!("COPY {} {}", input, input));
                    }
                }

                lines.push(format!("RUN {}", run_step.command));
                lines.push("".to_string());
                lines
            }
            Step::Copy(copy_step) => {
                if copy_step.is_download() {
                    vec![
                        format!("ADD {} {}", copy_step.source, copy_step.target),
                        "".to_string(),
                    ]
                } else {
                    vec![
                        format!("COPY {} {}", copy_step.source, copy_step.target),
                        "".to_string(),
                    ]
                }
            }
            Step::Env(env_step) => {
                let mut lines = vec![];
                for (key, value) in &env_step.variables {
                    lines.push(format!("ENV {}={}", key, value));
                    env.insert(key.clone(), value.clone());
                }
                if !lines.is_empty() {
                    lines.push("".to_string());
                }
                lines
            }
            Step::Path(path_step) => {
                let new_path = format!("{}:$PATH", path_step.path);
                env.insert("PATH".to_string(), new_path.clone());
                vec![format!("ENV PATH={}", new_path), "".to_string()]
            }
        }
    }

    /// Generate .dockerignore file content.
    fn generate_dockerignore(&self) -> String {
        [
            ".shipit",
            "Shipit",
            ".git",
            ".gitignore",
            "node_modules",
            "__pycache__",
            "*.pyc",
            ".DS_Store",
        ]
        .join("\n")
    }
}

impl BuildBackend for DockerBuildBackend {
    fn get_build_mount_path(&self, name: &str) -> PathBuf {
        // Absolute path inside container
        PathBuf::from("/").join(self.get_mount_path(name))
    }

    fn get_artifact_mount_path(&self, name: &str) -> PathBuf {
        // Host path for artifacts
        self.docker_out_path.join(self.get_mount_path(name))
    }

    fn execute_step(&mut self, _step: &Step, _env: &mut HashMap<String, String>) -> Result<()> {
        todo!("Docker backend uses Dockerfile generation, not direct step execution")
    }

    fn build(
        &mut self,
        name: &str,
        mut env: HashMap<String, String>,
        mounts: &[Mount],
        steps: &[Step],
    ) -> Result<()> {
        println!("🐳 Building '{}' with Docker", name);

        // Clean and create docker directory
        if self.docker_path.exists() {
            std::fs::remove_dir_all(&self.docker_path)
                .context("Failed to clean docker directory")?;
        }
        std::fs::create_dir_all(&self.docker_out_path)
            .context("Failed to create docker output directory")?;

        // Generate Dockerfile
        let mut dockerfile_lines = self.generate_dockerfile_header();

        // Convert steps to Dockerfile instructions
        for step in steps {
            let step_lines = self.step_to_dockerfile(step, &mut env);
            dockerfile_lines.extend(step_lines);
        }

        // Add export instructions for mounts
        dockerfile_lines.push("# Export artifacts".to_string());
        dockerfile_lines.push("FROM scratch AS export".to_string());
        for mount in mounts {
            let mount_path = self.get_build_mount_path(&mount.name);
            dockerfile_lines.push(format!(
                "COPY --from=build {} {}",
                mount_path.display(),
                self.get_mount_path(&mount.name).display()
            ));
        }
        dockerfile_lines.push("".to_string());

        let dockerfile_content = dockerfile_lines.join("\n");

        // Write Dockerfile
        std::fs::write(&self.docker_file_path, &dockerfile_content)
            .context("Failed to write Dockerfile")?;

        // Write .dockerignore
        let dockerignore_content = self.generate_dockerignore();
        std::fs::write(&self.docker_ignore_path, dockerignore_content)
            .context("Failed to write .dockerignore")?;

        // Generate image name
        let image_name = format!("shipit-{}", name);
        std::fs::write(&self.docker_name_path, &image_name)
            .context("Failed to write image name")?;

        // Print Dockerfile
        println!("\n📄 Dockerfile:");
        println!("{}", "=".repeat(60));
        for line in dockerfile_lines.iter() {
            println!("{}", line);
        }
        println!("{}", "=".repeat(60));
        println!();

        // Build with Docker
        println!("🔨 Building Docker image...");
        let mut cmd = std::process::Command::new(&self.docker_client);
        cmd.arg("build")
            .arg("-f")
            .arg(&self.docker_file_path)
            .arg("-t")
            .arg(&image_name)
            .arg("--platform")
            .arg("linux/amd64")
            .arg("--output")
            .arg(format!(
                "type=local,dest={}",
                self.docker_out_path.display()
            ))
            .arg("--target")
            .arg("export")
            .current_dir(&self.src_dir);

        // Add extra docker options
        if let Some(ref opts) = self.docker_opts {
            for opt in opts.split_whitespace() {
                cmd.arg(opt);
            }
        }

        cmd.arg(".");

        let status = cmd.status().context("Failed to execute docker build")?;

        if !status.success() {
            anyhow::bail!("Docker build failed");
        }

        // Save runtime PATH
        self.runtime_path = env.get("PATH").cloned();

        println!("✅ Docker build completed successfully");
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
        let backend = DockerBuildBackend::new(
            PathBuf::from("/test/src"),
            PathBuf::from("/test/assets"),
            None,
            None,
        );

        assert_eq!(backend.src_dir, PathBuf::from("/test/src"));
        assert_eq!(
            backend.docker_path,
            PathBuf::from("/test/src/.shipit/docker")
        );
        assert_eq!(backend.docker_client, "docker");
    }

    #[test]
    fn test_custom_docker_client() {
        let backend = DockerBuildBackend::new(
            PathBuf::from("/test/src"),
            PathBuf::from("/test/assets"),
            Some("podman".to_string()),
            None,
        );

        assert_eq!(backend.docker_client, "podman");
    }

    #[test]
    fn test_get_mount_path() {
        let backend = DockerBuildBackend::new(
            PathBuf::from("/test/src"),
            PathBuf::from("/test/assets"),
            None,
            None,
        );

        assert_eq!(backend.get_mount_path("app"), PathBuf::from("app"));
        assert_eq!(backend.get_mount_path("temp"), PathBuf::from("opt/temp"));
    }

    #[test]
    fn test_get_build_mount_path() {
        let backend = DockerBuildBackend::new(
            PathBuf::from("/test/src"),
            PathBuf::from("/test/assets"),
            None,
            None,
        );

        assert_eq!(backend.get_build_mount_path("app"), PathBuf::from("/app"));
        assert_eq!(
            backend.get_build_mount_path("temp"),
            PathBuf::from("/opt/temp")
        );
    }

    #[test]
    fn test_get_artifact_mount_path() {
        let backend = DockerBuildBackend::new(
            PathBuf::from("/test/src"),
            PathBuf::from("/test/assets"),
            None,
            None,
        );

        assert_eq!(
            backend.get_artifact_mount_path("app"),
            PathBuf::from("/test/src/.shipit/docker/out/app")
        );
        assert_eq!(
            backend.get_artifact_mount_path("temp"),
            PathBuf::from("/test/src/.shipit/docker/out/opt/temp")
        );
    }

    #[test]
    fn test_mise_config_php() {
        let config = DockerBuildBackend::get_mise_config("php").unwrap();
        assert_eq!(config.source, "ubi:adwinying/php");
        assert!(config.postinstall.is_none());
    }

    #[test]
    fn test_mise_config_composer() {
        let config = DockerBuildBackend::get_mise_config("composer").unwrap();
        assert_eq!(config.source, "ubi:composer/composer");
        assert!(config.postinstall.is_some());
    }

    #[test]
    fn test_mise_config_unknown() {
        let config = DockerBuildBackend::get_mise_config("unknown");
        assert!(config.is_none());
    }
}
