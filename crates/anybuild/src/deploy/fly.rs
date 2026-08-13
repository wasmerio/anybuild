//! Fly.io deployment adapter.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use toml_edit::{value, DocumentMut, Item, Table};

use crate::artifact::{ArtifactKind, RuntimeArtifact};
use crate::deploy::Deployer;
use crate::operation::OperationContext;
use crate::sdk::{DeployOutcome, DeployTarget, FlyOptions};

#[cfg(test)]
#[derive(Debug, Clone)]
struct CapturedCommand {
    command: String,
    args: Vec<String>,
    token: Option<String>,
}

pub(crate) struct FlyDeployer {
    options: FlyOptions,
    operation: OperationContext,
    #[cfg(test)]
    captured_commands: Vec<CapturedCommand>,
}

impl FlyDeployer {
    pub(crate) fn new(options: FlyOptions, operation: OperationContext) -> Self {
        let operation = options
            .token
            .as_deref()
            .map(|token| operation.with_secret(token))
            .unwrap_or(operation);
        Self {
            options,
            operation,
            #[cfg(test)]
            captured_commands: Vec::new(),
        }
    }

    fn binary(&self) -> &str {
        self.options.binary.as_deref().unwrap_or("flyctl")
    }

    fn config_path(&self, artifact_dir: &Path, context: &Path) -> Result<PathBuf> {
        if let Some(config) = &self.options.config {
            let path = if config.is_absolute() {
                config.clone()
            } else {
                context.join(config)
            };
            anyhow::ensure!(
                path.is_file(),
                "Fly config does not exist: {}",
                path.display()
            );
            return Ok(path);
        }

        let project_config = context.join("fly.toml");
        if project_config.is_file() {
            return Ok(project_config);
        }

        let app = self
            .options
            .app
            .as_deref()
            .filter(|app| !app.is_empty())
            .context("Fly deployment requires --fly-app when the project has no fly.toml")?;
        let port_path = artifact_dir.join("port");
        let port = std::fs::read_to_string(&port_path)
            .with_context(|| format!("reading Docker runtime port from {}", port_path.display()))?
            .trim()
            .parse::<i64>()
            .with_context(|| format!("invalid Docker runtime port in {}", port_path.display()))?;
        anyhow::ensure!(
            (1..=65_535).contains(&port),
            "Docker runtime port must be between 1 and 65535, found {port}"
        );

        let mut document = DocumentMut::new();
        document["app"] = value(app);
        let mut service = Table::new();
        service["internal_port"] = value(port);
        service["force_https"] = value(true);
        service["auto_stop_machines"] = value("stop");
        service["auto_start_machines"] = value(true);
        service["min_machines_running"] = value(0);
        document["http_service"] = Item::Table(service);

        let path = artifact_dir.join("fly.toml");
        std::fs::write(&path, document.to_string())?;
        Ok(path)
    }

    fn deploy_args(&self, artifact_dir: &Path, context: &Path, config: &Path) -> Vec<String> {
        let mut args = vec![
            "deploy".to_owned(),
            context.to_string_lossy().into_owned(),
            "--local-only".to_owned(),
            "--dockerfile".to_owned(),
            artifact_dir
                .join("Dockerfile")
                .to_string_lossy()
                .into_owned(),
            "--ignorefile".to_owned(),
            artifact_dir
                .join("Dockerfile.dockerignore")
                .to_string_lossy()
                .into_owned(),
            "--config".to_owned(),
            config.to_string_lossy().into_owned(),
        ];
        if let Some(app) = self.options.app.as_deref().filter(|app| !app.is_empty()) {
            args.extend(["--app".to_owned(), app.to_owned()]);
        }
        args.push("--yes".to_owned());
        args
    }

    fn run_command(&mut self, command: &str, args: &[String]) -> Result<()> {
        #[cfg(test)]
        {
            let _ = &self.operation;
            self.captured_commands.push(CapturedCommand {
                command: command.to_owned(),
                args: args.to_vec(),
                token: self.options.token.clone(),
            });
            Ok(())
        }
        #[cfg(not(test))]
        {
            let mut process = std::process::Command::new(command);
            self.operation.prepare_command(&mut process);
            process.args(args);
            if let Some(token) = &self.options.token {
                process.env("FLY_API_TOKEN", token);
            }
            let status = self
                .operation
                .command_status(&mut process)
                .with_context(|| format!("failed to run {command}"))?;
            anyhow::ensure!(
                status.success(),
                "Command {command} failed with exit code {:?}",
                status.code()
            );
            Ok(())
        }
    }
}

impl Deployer for FlyDeployer {
    fn platform_name(&self) -> &'static str {
        "Fly.io"
    }

    fn artifact_kind(&self) -> ArtifactKind {
        ArtifactKind::Docker
    }

    fn load_legacy_artifact(&self, _anybuild_dir: &Path) -> Result<RuntimeArtifact> {
        bail!("Fly.io deployment requires a Docker artifact; build with --runner=docker first")
    }

    fn deploy(
        &mut self,
        artifact: &RuntimeArtifact,
        target: DeployTarget,
    ) -> Result<DeployOutcome> {
        let (artifact_dir, _image, context) = artifact.docker_parts().with_context(|| {
            format!(
                "{} deployment requires a Docker artifact, found {:?}",
                self.platform_name(),
                artifact.kind()
            )
        })?;
        match target {
            DeployTarget::WriteConfig { .. } => {
                bail!("Fly.io deployment does not support Wasmer deployment configs")
            }
            DeployTarget::Publish { .. } => {
                let config = self.config_path(artifact_dir, context)?;
                let args = self.deploy_args(artifact_dir, context, &config);
                let binary = self.binary().to_owned();
                self.run_command(&binary, &args)?;
                Ok(DeployOutcome::Published {
                    owner: None,
                    name: self.options.app.clone(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn docker_artifact(root: &Path) -> RuntimeArtifact {
        let context = root.join("project");
        let directory = context.join(".anybuild/runner/docker");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("Dockerfile"), "FROM scratch\n").unwrap();
        std::fs::write(directory.join("Dockerfile.dockerignore"), "**\n").unwrap();
        std::fs::write(directory.join("port"), "3000").unwrap();
        RuntimeArtifact::Docker {
            directory,
            image: "example-api".to_owned(),
            context,
        }
    }

    #[test]
    fn deploy_generates_config_and_uses_local_docker_build() {
        let temporary = tempfile::tempdir().unwrap();
        let artifact = docker_artifact(temporary.path());
        let mut deployer = FlyDeployer::new(
            FlyOptions {
                app: Some("example-api".to_owned()),
                token: Some("secret-token".to_owned()),
                ..Default::default()
            },
            OperationContext::for_test(),
        );

        let outcome = deployer
            .deploy(
                &artifact,
                DeployTarget::Publish {
                    owner: None,
                    name: None,
                },
            )
            .unwrap();

        assert!(matches!(
            outcome,
            DeployOutcome::Published { name: Some(name), .. } if name == "example-api"
        ));
        let (directory, _, context) = artifact.docker_parts().unwrap();
        let config = std::fs::read_to_string(directory.join("fly.toml")).unwrap();
        assert!(config.contains("app = \"example-api\""));
        assert!(config.contains("internal_port = 3000"));
        let captured = deployer.captured_commands.last().unwrap();
        assert_eq!(captured.command, "flyctl");
        assert_eq!(captured.token.as_deref(), Some("secret-token"));
        assert_eq!(captured.args[0], "deploy");
        assert_eq!(captured.args[1], context.to_string_lossy());
        assert_eq!(
            captured.args,
            [
                "deploy",
                &context.to_string_lossy(),
                "--local-only",
                "--dockerfile",
                &directory.join("Dockerfile").to_string_lossy(),
                "--ignorefile",
                &directory.join("Dockerfile.dockerignore").to_string_lossy(),
                "--config",
                &directory.join("fly.toml").to_string_lossy(),
                "--app",
                "example-api",
                "--yes",
            ]
        );
    }

    #[test]
    fn deploy_reuses_project_fly_config_without_an_app_override() {
        let temporary = tempfile::tempdir().unwrap();
        let artifact = docker_artifact(temporary.path());
        let (_, _, context) = artifact.docker_parts().unwrap();
        let config_path = context.join("fly.toml");
        std::fs::write(&config_path, "app = \"configured-app\"\n").unwrap();
        let mut deployer = FlyDeployer::new(FlyOptions::default(), OperationContext::for_test());

        deployer
            .deploy(
                &artifact,
                DeployTarget::Publish {
                    owner: None,
                    name: None,
                },
            )
            .unwrap();

        let captured = deployer.captured_commands.last().unwrap();
        assert!(!captured.args.iter().any(|arg| arg == "--app"));
        let config_index = captured
            .args
            .iter()
            .position(|arg| arg == "--config")
            .unwrap();
        assert_eq!(
            captured.args[config_index + 1],
            config_path.to_string_lossy()
        );
    }
}
