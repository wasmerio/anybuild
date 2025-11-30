use std::path::Path;

use camino::Utf8PathBuf;

use crate::Result;
use crate::model::{CustomCommands, DependencySpec, DetectResult, MountSpec, ProviderPlan};
use crate::provider::{Provider, ProviderDescriptor, apply_custom_commands};

pub struct MkdocsProvider {
    path: Utf8PathBuf,
    custom_commands: CustomCommands,
}

impl MkdocsProvider {
    pub fn new(path: &Path, custom: &CustomCommands) -> Result<Self> {
        let path_buf = Utf8PathBuf::from_path_buf(path.to_path_buf())
            .map_err(|_| anyhow::anyhow!("Invalid UTF-8 path"))?;
        Ok(Self {
            path: path_buf,
            custom_commands: custom.clone(),
        })
    }
}

impl Provider for MkdocsProvider {
    fn name(&self) -> &'static str {
        "mkdocs"
    }

    fn plan(&self) -> Result<ProviderPlan> {
        let mut mounts = vec![MountSpec {
            name: "app".to_string(),
            attach_to_build: true,
            attach_to_serve: true,
        }];
        mounts.push(MountSpec {
            name: "temp".to_string(),
            attach_to_build: true,
            attach_to_serve: false,
        });

        let mut deps = vec![
            DependencySpec {
                name: "python".to_string(),
                env_var: Some("SHIPIT_PYTHON_VERSION".to_string()),
                default_version: Some("3.13".to_string()),
                architecture_var: None,
                alias: None,
                use_in_build: true,
                use_in_serve: true,
            },
            DependencySpec {
                name: "static-web-server".to_string(),
                env_var: Some("SHIPIT_SWS_VERSION".to_string()),
                default_version: Some("2.38.0".to_string()),
                architecture_var: None,
                alias: None,
                use_in_build: false,
                use_in_serve: true,
            },
        ];
        deps.push(DependencySpec {
            name: "mkdocs".to_string(),
            env_var: Some("SHIPIT_MKDOCS_VERSION".to_string()),
            default_version: Some("1.6.1".to_string()),
            architecture_var: None,
            alias: None,
            use_in_build: true,
            use_in_serve: false,
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
            platform: None,
            volumes: Vec::new(),
            declarations: Some(
                "mkdocs_version = getenv(\"SHIPIT_MKDOCS_VERSION\") or \"1.6.1\"".to_string(),
            ),
            dependencies: deps,
            build_steps: vec![
                "workdir(temp[\"build\"])".to_string(),
                "copy(\".\", \".\", ignore=[\".git\"])".to_string(),
                "run(\"uv run mkdocs build --site-dir={}\".format(app[\"build\"]), outputs=[\".\"], group=\"build\")"
                    .to_string(),
            ],
            prepare: None,
            services: Vec::new(),
            commands,
            env: None,
        })
    }
}

pub struct MkdocsDescriptor;

impl ProviderDescriptor for MkdocsDescriptor {
    fn detect(&self, path: &Path, custom: &CustomCommands) -> Option<DetectResult> {
        if exists(path, &["mkdocs.yml", "mkdocs.yaml"]) {
            return Some(DetectResult {
                name: "mkdocs".to_string(),
                score: 85,
            });
        }
        if let Some(build) = &custom.build {
            if build.starts_with("mkdocs ") {
                return Some(DetectResult {
                    name: "mkdocs".to_string(),
                    score: 85,
                });
            }
        }
        None
    }

    fn create(&self, path: &Path, custom: &CustomCommands) -> Result<Box<dyn Provider>> {
        Ok(Box::new(MkdocsProvider::new(path, custom)?))
    }

    fn name(&self) -> &'static str {
        "mkdocs"
    }
}

fn exists(path: &Path, names: &[&str]) -> bool {
    names.iter().any(|n| path.join(n).exists())
}
