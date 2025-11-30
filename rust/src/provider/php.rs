use std::collections::BTreeMap;
use std::path::Path;

use camino::Utf8PathBuf;

use crate::Result;
use crate::model::{CustomCommands, DependencySpec, DetectResult, MountSpec, ProviderPlan};
use crate::provider::{Provider, ProviderDescriptor, apply_custom_commands};

pub struct PhpProvider {
    path: Utf8PathBuf,
    custom_commands: CustomCommands,
    has_composer: bool,
}

impl PhpProvider {
    pub fn new(path: &Path, custom_commands: &CustomCommands) -> Result<Self> {
        let path_buf = Utf8PathBuf::from_path_buf(path.to_path_buf())
            .map_err(|_| anyhow::anyhow!("Invalid UTF-8 path"))?;
        let has_composer = path_buf.join("composer.json").exists()
            || path_buf.join("composer.lock").exists()
            || custom_commands
                .install
                .as_ref()
                .map(|s| s.starts_with("composer "))
                .unwrap_or(false);
        Ok(Self {
            path: path_buf,
            custom_commands: custom_commands.clone(),
            has_composer,
        })
    }

    pub(crate) fn dependencies_list(&self) -> Vec<DependencySpec> {
        let mut deps = vec![DependencySpec {
            name: "php".to_string(),
            env_var: Some("SHIPIT_PHP_VERSION".to_string()),
            default_version: Some("8.3".to_string()),
            architecture_var: Some("SHIPIT_PHP_ARCHITECTURE".to_string()),
            alias: None,
            use_in_build: true,
            use_in_serve: true,
        }];
        if self.has_composer {
            deps.push(DependencySpec {
                name: "composer".to_string(),
                env_var: None,
                default_version: None,
                architecture_var: None,
                alias: None,
                use_in_build: true,
                use_in_serve: false,
            });
            deps.push(DependencySpec {
                name: "bash".to_string(),
                env_var: None,
                default_version: None,
                architecture_var: None,
                alias: None,
                use_in_build: false,
                use_in_serve: true,
            });
        }
        deps
    }

    pub(crate) fn declarations_str(&self) -> Option<String> {
        if self.has_composer {
            Some("HOME = getenv(\"HOME\")\n".to_string())
        } else {
            None
        }
    }

    pub(crate) fn base_build_steps(&self) -> Vec<String> {
        let mut steps = vec!["workdir(app[\"build\"])".to_string()];
        if self.path.join("php.ini").exists() {
            steps.push("copy(\"php.ini\", \"{}/php.ini\".format(assets[\"build\"]))".to_string());
        } else {
            steps.push(
                "copy(\"php/php.ini\", \"{}/php.ini\".format(assets[\"build\"]), base=\"assets\")"
                    .to_string(),
            );
        }

        if self.has_composer {
            steps.push("env(HOME=HOME, COMPOSER_FUND=\"0\")".to_string());
            steps.push("run(\"composer install --optimize-autoloader --no-scripts --no-interaction\", inputs=[\"composer.json\", \"composer.lock\"], outputs=[\".\"], group=\"install\")".to_string());
        }

        steps.push("copy(\".\", \".\", ignore=[\".git\"])".to_string());
        steps
    }

    pub(crate) fn base_commands(&self) -> BTreeMap<String, String> {
        let mut commands = BTreeMap::new();
        let public_index = self.path.join("public").join("index.php");
        let app_index = self.path.join("app").join("index.php");
        if public_index.exists() {
            commands.insert(
                "start".to_string(),
                "\"php -S localhost:{} -t {}/public\".format(PORT, app[\"serve\"])".to_string(),
            );
        } else if app_index.exists() {
            commands.insert(
                "start".to_string(),
                "\"php -S localhost:{} -t {}/app\".format(PORT, app[\"serve\"])".to_string(),
            );
        } else if self.path.join("index.php").exists() {
            commands.insert(
                "start".to_string(),
                "\"php -S localhost:{} -t {}\".format(PORT, app[\"serve\"])".to_string(),
            );
        } else {
            commands.insert(
                "start".to_string(),
                "\"php -S localhost:{} -t {}\".format(PORT, app[\"serve\"])".to_string(),
            );
        }
        commands
    }

    pub(crate) fn env_map(&self) -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        map.insert(
            "PHP_INI_SCAN_DIR".to_string(),
            "\"{}\".format(assets[\"serve\"])".to_string(),
        );
        map
    }

    pub(crate) fn commands(&self) -> Result<BTreeMap<String, String>> {
        apply_custom_commands(&self.custom_commands, self.base_commands())
    }
}

impl Provider for PhpProvider {
    fn name(&self) -> &'static str {
        "php"
    }

    fn plan(&self) -> Result<ProviderPlan> {
        Ok(ProviderPlan {
            serve_name: self
                .path
                .file_name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "app".to_string()),
            cwd: None,
            provider: self.name().to_string(),
            mounts: vec![
                MountSpec {
                    name: "app".to_string(),
                    attach_to_build: true,
                    attach_to_serve: true,
                },
                MountSpec {
                    name: "assets".to_string(),
                    attach_to_build: true,
                    attach_to_serve: true,
                },
            ],
            platform: None,
            volumes: Vec::new(),
            declarations: self.declarations_str(),
            dependencies: self.dependencies_list(),
            build_steps: self.base_build_steps(),
            prepare: None,
            services: Vec::new(),
            commands: self.commands()?,
            env: Some(self.env_map()),
        })
    }
}

pub struct PhpDescriptor;

impl ProviderDescriptor for PhpDescriptor {
    fn detect(&self, path: &Path, custom: &CustomCommands) -> Option<DetectResult> {
        let has_composer = path.join("composer.json").exists();
        if has_composer && path.join("public/index.php").exists() {
            return Some(DetectResult {
                name: "php".to_string(),
                score: 60,
            });
        }
        if path.join("index.php").exists() || path.join("public/index.php").exists() {
            return Some(DetectResult {
                name: "php".to_string(),
                score: 10,
            });
        }
        if let Some(start) = &custom.start {
            if start.starts_with("php ") {
                return Some(DetectResult {
                    name: "php".to_string(),
                    score: 70,
                });
            }
        }
        if let Some(install) = &custom.install {
            if install.starts_with("composer ") {
                return Some(DetectResult {
                    name: "php".to_string(),
                    score: 30,
                });
            }
        }
        None
    }

    fn create(&self, path: &Path, custom: &CustomCommands) -> Result<Box<dyn Provider>> {
        Ok(Box::new(PhpProvider::new(path, custom)?))
    }

    fn name(&self) -> &'static str {
        "php"
    }
}
