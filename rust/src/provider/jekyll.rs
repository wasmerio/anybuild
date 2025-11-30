use std::path::Path;

use camino::Utf8PathBuf;

use crate::Result;
use crate::model::{CustomCommands, DependencySpec, DetectResult, MountSpec, ProviderPlan};
use crate::provider::{Provider, ProviderDescriptor, apply_custom_commands};

pub struct JekyllProvider {
    path: Utf8PathBuf,
    custom_commands: CustomCommands,
}

impl JekyllProvider {
    pub fn new(path: &Path, custom: &CustomCommands) -> Result<Self> {
        let path_buf = Utf8PathBuf::from_path_buf(path.to_path_buf())
            .map_err(|_| anyhow::anyhow!("Invalid UTF-8 path"))?;
        Ok(Self {
            path: path_buf,
            custom_commands: custom.clone(),
        })
    }
}

impl Provider for JekyllProvider {
    fn name(&self) -> &'static str {
        "jekyll"
    }

    fn plan(&self) -> Result<ProviderPlan> {
        let mut build_steps: Vec<String> = Vec::new();
        let gemfile = self.path.join("Gemfile").exists();
        build_steps.push("workdir(temp[\"build\"])".to_string());
        build_steps.push("copy(\".\", ignore=[\".git\"])".to_string());
        if gemfile {
            if self.path.join("Gemfile.lock").exists() {
                build_steps.push("copy(\"Gemfile.lock\")".to_string());
            }
            build_steps
                .push("run(\"bundle install\", inputs=[\"Gemfile\"], group=\"build\")".to_string());
        } else {
            build_steps.push("run(\"gem install jekyll\", group=\"build\")".to_string());
        }
        build_steps.push(
            "run(\"jekyll build --destination={}\".format(app[\"build\"]), group=\"build\")"
                .to_string(),
        );

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
            platform: None,
            volumes: Vec::new(),
            declarations: Some(
                "jekyll_version = getenv(\"SHIPIT_JEKYLL_VERSION\") or \"1.6.1\"".to_string(),
            ),
            dependencies: vec![
                DependencySpec {
                    name: "ruby".to_string(),
                    env_var: Some("SHIPIT_RUBY_VERSION".to_string()),
                    default_version: None,
                    architecture_var: None,
                    alias: None,
                    use_in_build: true,
                    use_in_serve: false,
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
            ],
            build_steps,
            prepare: None,
            services: Vec::new(),
            commands,
            env: None,
        })
    }
}

pub struct JekyllDescriptor;

impl ProviderDescriptor for JekyllDescriptor {
    fn detect(&self, path: &Path, custom: &CustomCommands) -> Option<DetectResult> {
        if exists(path, &["_config.yml", "_config.yaml"]) {
            if path.join("Gemfile").exists() {
                return Some(DetectResult {
                    name: "jekyll".to_string(),
                    score: 85,
                });
            }
            return Some(DetectResult {
                name: "jekyll".to_string(),
                score: 40,
            });
        }
        if let Some(build) = &custom.build {
            if build.starts_with("jekyll ") {
                return Some(DetectResult {
                    name: "jekyll".to_string(),
                    score: 85,
                });
            }
        }
        None
    }

    fn create(&self, path: &Path, custom: &CustomCommands) -> Result<Box<dyn Provider>> {
        Ok(Box::new(JekyllProvider::new(path, custom)?))
    }

    fn name(&self) -> &'static str {
        "jekyll"
    }
}

fn exists(path: &Path, names: &[&str]) -> bool {
    names.iter().any(|n| path.join(n).exists())
}
