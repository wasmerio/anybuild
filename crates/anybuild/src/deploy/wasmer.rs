//! Wasmer Edge deployment adapter.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_yaml::Value as YamlValue;
use sha2::Digest;

use crate::artifact::{ArtifactKind, RuntimeArtifact};
use crate::build::report::console_print;
use crate::deploy::Deployer;
use crate::operation::OperationContext;
use crate::sdk::{DeployOutcome, DeployTarget, WasmerOptions};
use crate::wasmer::{dump_yaml_sorted, path_str, yaml_str};

#[cfg(test)]
#[derive(Debug, Clone)]
struct CapturedCommand {
    command: String,
    extra_args: Vec<String>,
}

pub(crate) struct WasmerDeployer {
    options: WasmerOptions,
    operation: OperationContext,
    #[cfg(test)]
    captured_commands: Vec<CapturedCommand>,
}

impl WasmerDeployer {
    pub(crate) fn new(options: WasmerOptions, operation: OperationContext) -> Self {
        Self {
            options,
            operation,
            #[cfg(test)]
            captured_commands: Vec::new(),
        }
    }

    fn binary(&self) -> &str {
        self.options.binary.as_deref().unwrap_or("wasmer")
    }

    fn update_app_yaml(
        &self,
        artifact_dir: &Path,
        app_owner: Option<&str>,
        app_name: Option<&str>,
    ) -> Result<()> {
        let app_owner = app_owner.filter(|owner| !owner.is_empty());
        let app_name = app_name.filter(|name| !name.is_empty());
        if app_owner.is_none() && app_name.is_none() {
            return Ok(());
        }
        let app_yaml_path = artifact_dir.join("app.yaml");
        if !app_yaml_path.exists() {
            return Ok(());
        }
        let text = std::fs::read_to_string(&app_yaml_path)?;
        let mut yaml_config = match serde_yaml::from_str::<YamlValue>(&text)? {
            YamlValue::Mapping(map) => map,
            YamlValue::Null => serde_yaml::Mapping::new(),
            _ => bail!("app.yaml must be a mapping"),
        };
        let mut changed = false;
        if let Some(owner) = app_owner {
            if yaml_config.get(yaml_str("owner")) != Some(&yaml_str(owner)) {
                yaml_config.insert(yaml_str("owner"), yaml_str(owner));
                changed = true;
            }
        }
        if let Some(name) = app_name {
            if yaml_config.get(yaml_str("name")) != Some(&yaml_str(name)) {
                yaml_config.insert(yaml_str("name"), yaml_str(name));
                changed = true;
            }
        }
        if changed {
            std::fs::write(
                &app_yaml_path,
                dump_yaml_sorted(&YamlValue::Mapping(yaml_config))?,
            )?;
        }
        Ok(())
    }

    fn deploy_args(
        &self,
        artifact_dir: &Path,
        app_owner: Option<&str>,
        app_name: Option<&str>,
    ) -> Vec<String> {
        let app_owner = app_owner.filter(|owner| !owner.is_empty());
        let app_name = app_name.filter(|name| !name.is_empty());
        let mut extra_args = Vec::new();
        if let Some(registry) = &self.options.registry {
            extra_args.extend(["--registry".to_owned(), registry.clone()]);
        }
        if let Some(token) = &self.options.token {
            extra_args.extend(["--token".to_owned(), token.clone()]);
        }
        if let Some(owner) = app_owner {
            extra_args.extend(["--owner".to_owned(), owner.to_owned()]);
        }
        if let Some(name) = app_name {
            extra_args.extend(["--app-name".to_owned(), name.to_owned()]);
        }
        let mut args = vec![
            "deploy".to_owned(),
            "--publish-package".to_owned(),
            "--dir".to_owned(),
            path_str(artifact_dir),
        ];
        if app_owner.is_some() && app_name.is_some() {
            args.push("--non-interactive".to_owned());
        }
        args.extend(extra_args);
        args
    }

    fn run_command(&mut self, command: &str, extra_args: &[String]) -> Result<()> {
        #[cfg(test)]
        {
            self.captured_commands.push(CapturedCommand {
                command: command.to_owned(),
                extra_args: extra_args.to_vec(),
            });
            Ok(())
        }
        #[cfg(not(test))]
        {
            let mut process = std::process::Command::new(command);
            self.operation.prepare_command(&mut process);
            process.args(extra_args);
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

    fn write_deploy_config(&mut self, artifact_dir: &Path, config_path: &Path) -> Result<()> {
        let package_webc_path = artifact_dir.join("package.webc");
        let app_yaml_path = artifact_dir.join("app.yaml");
        if let Some(parent) = package_webc_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let binary = self.binary().to_owned();
        self.run_command(
            &binary,
            &[
                "package".to_owned(),
                "build".to_owned(),
                path_str(artifact_dir),
                "--out".to_owned(),
                path_str(&package_webc_path),
            ],
        )?;
        let contents = std::fs::read(&package_webc_path)
            .with_context(|| format!("reading {}", package_webc_path.display()))?;
        let sha256 = format!("{:x}", sha2::Sha256::digest(&contents));
        let payload = format!(
            "{{\"app_yaml_path\": {}, \"package_webc_path\": {}, \"package_webc_size\": {}, \"package_webc_sha256\": {}}}",
            serde_json::to_string(&path_str(&absolute_path(&app_yaml_path)))?,
            serde_json::to_string(&path_str(&absolute_path(&package_webc_path)))?,
            contents.len(),
            serde_json::to_string(&sha256)?,
        );
        std::fs::write(config_path, payload)?;
        console_print(
            &self.operation,
            &format!("\nSaved deploy config to {}", config_path.display()),
        );
        Ok(())
    }

    fn publish(
        &mut self,
        artifact_dir: &Path,
        owner: Option<&str>,
        name: Option<&str>,
    ) -> Result<()> {
        self.update_app_yaml(artifact_dir, owner, name)?;
        let binary = self.binary().to_owned();
        let args = self.deploy_args(artifact_dir, owner, name);
        self.run_command(&binary, &args)
    }
}

impl Deployer for WasmerDeployer {
    fn platform_name(&self) -> &'static str {
        "Wasmer"
    }

    fn artifact_kind(&self) -> ArtifactKind {
        ArtifactKind::Wasmer
    }

    fn load_legacy_artifact(&self, anybuild_dir: &Path) -> Result<RuntimeArtifact> {
        Ok(RuntimeArtifact::Wasmer {
            directory: anybuild_dir.join("wasmer"),
        })
    }

    fn deploy(
        &mut self,
        artifact: &RuntimeArtifact,
        target: DeployTarget,
    ) -> Result<DeployOutcome> {
        let artifact_dir = artifact.wasmer_directory().with_context(|| {
            format!(
                "{} deployment requires a Wasmer artifact, found {:?}",
                self.platform_name(),
                artifact.kind()
            )
        })?;
        match target {
            DeployTarget::WriteConfig { path } => {
                self.write_deploy_config(artifact_dir, &path)?;
                Ok(DeployOutcome::ConfigWritten { path })
            }
            DeployTarget::Publish { owner, name } => {
                self.publish(artifact_dir, owner.as_deref(), name.as_deref())?;
                Ok(DeployOutcome::Published { owner, name })
            }
        }
    }
}

fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact(root: &Path) -> RuntimeArtifact {
        RuntimeArtifact::Wasmer {
            directory: root.join("wasmer"),
        }
    }

    #[test]
    fn deploy_omits_unspecified_app_identity() {
        let temporary = tempfile::tempdir().unwrap();
        let artifact = artifact(temporary.path());
        let artifact_dir = artifact.wasmer_directory().unwrap();
        let mut deployer =
            WasmerDeployer::new(WasmerOptions::default(), OperationContext::for_test());

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
        assert_eq!(captured.command, "wasmer");
        assert_eq!(
            captured.extra_args,
            [
                "deploy",
                "--publish-package",
                "--dir",
                &path_str(artifact_dir),
            ]
        );
    }

    #[test]
    fn deploy_is_non_interactive_with_complete_app_identity() {
        let temporary = tempfile::tempdir().unwrap();
        let artifact = artifact(temporary.path());
        let artifact_dir = artifact.wasmer_directory().unwrap();
        let mut deployer =
            WasmerDeployer::new(WasmerOptions::default(), OperationContext::for_test());

        deployer
            .deploy(
                &artifact,
                DeployTarget::Publish {
                    owner: Some("acme".to_owned()),
                    name: Some("blog".to_owned()),
                },
            )
            .unwrap();

        let captured = deployer.captured_commands.last().unwrap();
        assert_eq!(
            captured.extra_args,
            [
                "deploy",
                "--publish-package",
                "--dir",
                &path_str(artifact_dir),
                "--non-interactive",
                "--owner",
                "acme",
                "--app-name",
                "blog",
            ]
        );
    }

    #[test]
    fn deploy_updates_app_identity_and_connection_arguments() {
        let temporary = tempfile::tempdir().unwrap();
        let artifact = artifact(temporary.path());
        let artifact_dir = artifact.wasmer_directory().unwrap();
        std::fs::create_dir_all(artifact_dir).unwrap();
        std::fs::write(
            artifact_dir.join("app.yaml"),
            "kind: wasmer.io/App.v0\nowner: old\n",
        )
        .unwrap();
        let mut deployer = WasmerDeployer::new(
            WasmerOptions {
                registry: Some("registry.example".to_owned()),
                token: Some("secret".to_owned()),
                ..Default::default()
            },
            OperationContext::for_test(),
        );

        deployer
            .deploy(
                &artifact,
                DeployTarget::Publish {
                    owner: Some("acme".to_owned()),
                    name: Some("blog".to_owned()),
                },
            )
            .unwrap();

        let app_yaml = std::fs::read_to_string(artifact_dir.join("app.yaml")).unwrap();
        assert!(app_yaml.contains("owner: acme"));
        assert!(app_yaml.contains("name: blog"));
        let captured = deployer.captured_commands.last().unwrap();
        assert!(captured
            .extra_args
            .windows(2)
            .any(|args| args == ["--registry", "registry.example"]));
        assert!(captured
            .extra_args
            .windows(2)
            .any(|args| args == ["--token", "secret"]));
    }
}
