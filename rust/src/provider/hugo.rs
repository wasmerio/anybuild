use std::path::Path;

use camino::Utf8PathBuf;

use crate::Result;
use crate::model::{CustomCommands, DependencySpec, DetectResult, MountSpec, ProviderPlan};
use crate::provider::{Provider, ProviderDescriptor, apply_custom_commands};

pub struct HugoProvider {
    path: Utf8PathBuf,
    custom_commands: CustomCommands,
}

impl HugoProvider {
    pub fn new(path: &Path, custom: &CustomCommands) -> Result<Self> {
        let path_buf = Utf8PathBuf::from_path_buf(path.to_path_buf())
            .map_err(|_| anyhow::anyhow!("Invalid UTF-8 path"))?;
        Ok(Self {
            path: path_buf,
            custom_commands: custom.clone(),
        })
    }
}

impl Provider for HugoProvider {
    fn name(&self) -> &'static str {
        "hugo"
    }

    fn platform(&self) -> Option<&str> {
        Some("hugo")
    }

    fn plan(&self) -> Result<ProviderPlan> {
        let mut deps = vec![DependencySpec {
            name: "hugo".to_string(),
            env_var: Some("SHIPIT_HUGO_VERSION".to_string()),
            default_version: Some("0.149.0".to_string()),
            architecture_var: None,
            alias: None,
            use_in_build: true,
            use_in_serve: false,
        }];
        deps.push(DependencySpec {
            name: "static-web-server".to_string(),
            env_var: Some("SHIPIT_SWS_VERSION".to_string()),
            default_version: Some("2.38.0".to_string()),
            architecture_var: None,
            alias: None,
            use_in_build: false,
            use_in_serve: true,
        });

        let mut mounts = vec![MountSpec {
            name: "temp".to_string(),
            attach_to_build: true,
            attach_to_serve: false,
        }];
        mounts.push(MountSpec {
            name: "app".to_string(),
            attach_to_build: true,
            attach_to_serve: true,
        });

        let mut commands = std::collections::BTreeMap::new();
        commands.insert(
            "start".to_string(),
            "\"static-web-server --root={} --log-level=info --port={}\".format(app[\"serve\"], PORT)".to_string(),
        );
        let commands = apply_custom_commands(&self.custom_commands, commands)?;

        Ok(ProviderPlan {
            serve_name: self
                .path
                .file_name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "app".to_string()),
            cwd: None,
            provider: self.name().to_string(),
            mounts,
            platform: self.platform().map(|s| s.to_string()),
            volumes: Vec::new(),
            declarations: None,
            dependencies: deps,
            build_steps: vec![
                "workdir(temp[\"build\"])".to_string(),
                "copy(\".\", \".\", ignore=[\".git\"])".to_string(),
                "run(\"hugo build --destination={}\".format(app[\"build\"]), group=\"build\")"
                    .to_string(),
            ],
            prepare: None,
            services: Vec::new(),
            commands,
            env: None,
        })
    }
}

pub struct HugoDescriptor;

impl ProviderDescriptor for HugoDescriptor {
    fn detect(&self, path: &Path, _custom: &CustomCommands) -> Option<DetectResult> {
        if exists(path, &["hugo.toml", "hugo.json", "hugo.yaml", "hugo.yml"]) {
            return Some(DetectResult {
                name: "hugo".to_string(),
                score: 80,
            });
        }
        if exists(
            path,
            &["config.toml", "config.json", "config.yaml", "config.yml"],
        ) && path.join("content").exists()
            && (path.join("static").exists() || path.join("themes").exists())
        {
            return Some(DetectResult {
                name: "hugo".to_string(),
                score: 40,
            });
        }
        None
    }

    fn create(&self, path: &Path, custom: &CustomCommands) -> Result<Box<dyn Provider>> {
        Ok(Box::new(HugoProvider::new(path, custom)?))
    }

    fn name(&self) -> &'static str {
        "hugo"
    }
}

fn exists(path: &Path, names: &[&str]) -> bool {
    names.iter().any(|n| path.join(n).exists())
}
