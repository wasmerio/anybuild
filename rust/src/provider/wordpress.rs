use std::path::Path;

use camino::Utf8PathBuf;

use crate::Result;
use crate::model::{
    CustomCommands, DependencySpec, DetectResult, MountSpec, ProviderPlan, ServiceProvider,
    ServiceSpec, VolumeSpec,
};
use crate::provider::php::PhpProvider;
use crate::provider::{Provider, ProviderDescriptor};

pub struct WordPressProvider {
    path: Utf8PathBuf,
    php: PhpProvider,
}

impl WordPressProvider {
    pub fn new(path: &Path, custom_commands: &CustomCommands) -> Result<Self> {
        let php = PhpProvider::new(path, custom_commands)?;
        let path_buf = Utf8PathBuf::from_path_buf(path.to_path_buf())
            .map_err(|_| anyhow::anyhow!("Invalid UTF-8 path"))?;
        Ok(Self {
            path: path_buf,
            php,
        })
    }
}

impl Provider for WordPressProvider {
    fn name(&self) -> &'static str {
        "wordpress"
    }

    fn plan(&self) -> Result<ProviderPlan> {
        let mut dependencies = self.php.dependencies_list();
        let has_bash = dependencies.iter().any(|d| d.name == "bash");
        if !has_bash {
            dependencies.push(DependencySpec {
                name: "bash".to_string(),
                env_var: None,
                default_version: None,
                architecture_var: None,
                alias: None,
                use_in_build: false,
                use_in_serve: true,
            });
        }

        let mut declarations = self.php.declarations_str().unwrap_or_default();
        declarations.push_str(
            "wp_cli_version = getenv(\"SHIPIT_WPCLI_VERSION\")\n\
             if wp_cli_version:\n\
                 wp_cli_download_url = f\"https://github.com/wp-cli/wp-cli/releases/download/v{wp_cli_version}/wp-cli-{wp_cli_version}.phar\"\n\
             else:\n\
                 wp_cli_download_url = \"https://raw.githubusercontent.com/wp-cli/builds/gh-pages/phar/wp-cli.phar\"\n",
        );

        let mut build_steps = vec![
            "copy(wp_cli_download_url, \"{}/wp-cli.phar\".format(assets[\"build\"]))".to_string(),
            "copy(\"wordpress/install.sh\", \"{}/setup-wp.sh\".format(assets[\"build\"]), base=\"assets\")"
                .to_string(),
        ];
        if !self.path.join("wp-config.php").exists() {
            build_steps.push(
                "copy(\"wordpress/wp-config.php\", \"{}/wp-config.php\".format(app[\"build\"]), base=\"assets\")"
                    .to_string(),
            );
        }
        build_steps.extend(self.php.base_build_steps());

        let mut commands = self.php.commands()?;
        if !commands.contains_key("after_deploy") {
            commands.insert(
                "after_deploy".to_string(),
                "\"bash {}/setup-wp.sh\".format(assets[\"serve\"])".to_string(),
            );
        }

        let mut env = self.php.env_map();
        env.insert("PAGER".to_string(), "\"cat\"".to_string());

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
            volumes: vec![VolumeSpec {
                name: "wp-content".to_string(),
                serve_path: "\"{}/wp-content/\".format(app[\"serve\"])".to_string(),
                var_name: Some("wp_content".to_string()),
            }],
            declarations: Some(declarations),
            dependencies,
            build_steps,
            prepare: None,
            services: vec![ServiceSpec {
                name: "database".to_string(),
                provider: ServiceProvider::Mysql,
            }],
            commands,
            env: Some(env),
        })
    }
}

pub struct WordPressDescriptor;

impl ProviderDescriptor for WordPressDescriptor {
    fn detect(&self, path: &Path, _custom: &CustomCommands) -> Option<DetectResult> {
        if path.join("wp-content").exists()
            && path.join("index.php").exists()
            && path.join("wp-load.php").exists()
        {
            return Some(DetectResult {
                name: "wordpress".to_string(),
                score: 80,
            });
        }
        None
    }

    fn create(&self, path: &Path, custom: &CustomCommands) -> Result<Box<dyn Provider>> {
        Ok(Box::new(WordPressProvider::new(path, custom)?))
    }

    fn name(&self) -> &'static str {
        "wordpress"
    }
}
