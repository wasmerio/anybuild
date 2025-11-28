use std::fs;
use std::path::Path;

use camino::Utf8PathBuf;
use serde::Deserialize;

use crate::Result;
use crate::model::{CustomCommands, DependencySpec, DetectResult, MountSpec, ProviderPlan};
use crate::provider::{Provider, ProviderDescriptor};

#[derive(Debug, Default, Deserialize)]
struct StaticConfig {
    root: Option<String>,
}

pub struct StaticFileProvider {
    path: Utf8PathBuf,
    pub custom_commands: CustomCommands,
    config: Option<StaticConfig>,
}

impl StaticFileProvider {
    pub fn new(path: &Path, custom_commands: &CustomCommands) -> Result<Self> {
        let path_buf = Utf8PathBuf::from_path_buf(path.to_path_buf())
            .map_err(|_| anyhow::anyhow!("Invalid UTF-8 path"))?;
        let mut config = None;
        let staticfile = path_buf.join("Staticfile");
        if staticfile.exists() {
            let text = fs::read_to_string(&staticfile)?;
            config = serde_yaml::from_str(&text).ok();
        }
        Ok(Self {
            path: path_buf,
            custom_commands: custom_commands.clone(),
            config,
        })
    }
}

impl Provider for StaticFileProvider {
    fn name(&self) -> &'static str {
        "staticfile"
    }

    fn plan(&self) -> Result<ProviderPlan> {
        let root = self
            .config
            .as_ref()
            .and_then(|c| c.root.clone())
            .unwrap_or_else(|| ".".to_string());

        let build_steps = vec![
            "workdir(app[\"build\"])".to_string(),
            format!(
                "copy({}, \".\", ignore=[\".git\"])",
                serde_json::to_string(&root)?
            ),
        ];

        Ok(ProviderPlan {
            serve_name: self
                .path
                .file_name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "app".to_string()),
            provider: self.name().to_string(),
            mounts: vec![MountSpec {
                name: "app".to_string(),
                attach_to_build: true,
                attach_to_serve: true,
            }],
            volumes: Vec::new(),
            declarations: None,
            dependencies: vec![DependencySpec {
                name: "static-web-server".to_string(),
                env_var: Some("SHIPIT_SWS_VERSION".to_string()),
                default_version: Some("2.38.0".to_string()),
                architecture_var: None,
                alias: None,
                use_in_build: false,
                use_in_serve: true,
            }],
            build_steps,
            prepare: None,
            services: Vec::new(),
            commands: {
                let mut map = std::collections::BTreeMap::new();
                map.insert(
                    "start".to_string(),
                    format!(
                        "\"static-web-server --root={{}} --log-level=info --port={{}}\".format(app[\"serve\"], PORT)"
                    ),
                );
                map
            },
            env: None,
            platform: None,
        })
    }
}

pub struct StaticFileDescriptor;

impl ProviderDescriptor for StaticFileDescriptor {
    fn detect(&self, path: &Path, custom: &CustomCommands) -> Option<DetectResult> {
        let has_staticfile = path.join("Staticfile").exists();
        let has_index = path.join("index.html").exists();
        let has_package = path.join("package.json").exists();
        let has_py = path.join("pyproject.toml").exists();
        let has_composer = path.join("composer.json").exists();

        if has_staticfile {
            return Some(DetectResult {
                name: "staticfile".to_string(),
                score: 50,
            });
        }
        if has_index && !has_package && !has_py && !has_composer {
            return Some(DetectResult {
                name: "staticfile".to_string(),
                score: 10,
            });
        }
        if let Some(start) = &custom.start {
            if start.starts_with("static-web-server ") {
                return Some(DetectResult {
                    name: "staticfile".to_string(),
                    score: 70,
                });
            }
        }
        None
    }

    fn create(&self, path: &Path, custom: &CustomCommands) -> Result<Box<dyn Provider>> {
        Ok(Box::new(StaticFileProvider::new(path, custom)?))
    }

    fn name(&self) -> &'static str {
        "staticfile"
    }
}
