use std::path::Path;

use camino::Utf8PathBuf;

use crate::Result;
use crate::model::{
    CustomCommands, DependencySpec, DetectResult, MountSpec, ProviderPlan, ServiceSpec,
};
use crate::provider::{Provider, ProviderDescriptor, apply_custom_commands};

pub struct LaravelProvider {
    path: Utf8PathBuf,
    custom_commands: CustomCommands,
}

impl LaravelProvider {
    pub fn new(path: &Path, custom_commands: &CustomCommands) -> Result<Self> {
        let path_buf = Utf8PathBuf::from_path_buf(path.to_path_buf())
            .map_err(|_| anyhow::anyhow!("Invalid UTF-8 path"))?;
        Ok(Self {
            path: path_buf,
            custom_commands: custom_commands.clone(),
        })
    }
}

impl Provider for LaravelProvider {
    fn name(&self) -> &'static str {
        "laravel"
    }

    fn platform(&self) -> Option<&str> {
        Some("laravel")
    }

    fn plan(&self) -> Result<ProviderPlan> {
        let mut commands = std::collections::BTreeMap::new();
        commands.insert(
            "start".to_string(),
            "f\"php -S localhost:{PORT} -t public\"".to_string(),
        );
        commands.insert(
            "after_deploy".to_string(),
            "\"php artisan migrate\"".to_string(),
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
            mounts: vec![MountSpec {
                name: "app".to_string(),
                attach_to_build: true,
                attach_to_serve: true,
            }],
            platform: self.platform().map(|s| s.to_string()),
            volumes: Vec::new(),
            declarations: Some("HOME = getenv(\"HOME\")".to_string()),
            dependencies: vec![
                DependencySpec {
                    name: "php".to_string(),
                    env_var: Some("SHIPIT_PHP_VERSION".to_string()),
                    default_version: Some("8.3".to_string()),
                    architecture_var: None,
                    alias: None,
                    use_in_build: true,
                    use_in_serve: true,
                },
                DependencySpec {
                    name: "composer".to_string(),
                    env_var: None,
                    default_version: None,
                    architecture_var: None,
                    alias: None,
                    use_in_build: true,
                    use_in_serve: false,
                },
                DependencySpec {
                    name: "pie".to_string(),
                    env_var: None,
                    default_version: None,
                    architecture_var: None,
                    alias: None,
                    use_in_build: true,
                    use_in_serve: false,
                },
                DependencySpec {
                    name: "pnpm".to_string(),
                    env_var: None,
                    default_version: None,
                    architecture_var: None,
                    alias: None,
                    use_in_build: true,
                    use_in_serve: false,
                },
                DependencySpec {
                    name: "bash".to_string(),
                    env_var: None,
                    default_version: None,
                    architecture_var: None,
                    alias: None,
                    use_in_build: false,
                    use_in_serve: true,
                },
            ],
            build_steps: vec![
                "env(HOME=HOME, COMPOSER_FUND=\"0\")".to_string(),
                "workdir(app[\"build\"])".to_string(),
                "run(\"pie install php/pdo_pgsql\")".to_string(),
                "run(\"composer install --optimize-autoloader --no-scripts --no-interaction\", inputs=[\"composer.json\", \"composer.lock\", \"artisan\"], outputs=[\".\"], group=\"install\")".to_string(),
                "run(\"pnpm install\", inputs=[\"package.json\", \"package-lock.json\"], outputs=[\".\"], group=\"install\")".to_string(),
                "copy(\".\", \".\", ignore=[\".git\"])".to_string(),
                "run(\"pnpm run build\", outputs=[\".\"], group=\"build\")".to_string(),
            ],
            prepare: Some(vec![
                "workdir(app[\"serve\"])".to_string(),
                "run(\"mkdir -p storage/framework/{sessions,views,cache,testing} storage/logs bootstrap/cache\")".to_string(),
                "run(\"php artisan config:cache\")".to_string(),
                "run(\"php artisan event:cache\")".to_string(),
                "run(\"php artisan route:cache\")".to_string(),
                "run(\"php artisan view:cache\")".to_string(),
            ]),
            services: vec![ServiceSpec {
                name: "database".to_string(),
                provider: crate::model::ServiceProvider::Mysql,
            }],
            commands,
            env: None,
        })
    }
}

pub struct LaravelDescriptor;

impl ProviderDescriptor for LaravelDescriptor {
    fn detect(&self, path: &Path, _custom: &CustomCommands) -> Option<DetectResult> {
        if path.join("artisan").exists() && path.join("composer.json").exists() {
            return Some(DetectResult {
                name: "laravel".to_string(),
                score: 95,
            });
        }
        None
    }

    fn create(&self, path: &Path, custom: &CustomCommands) -> Result<Box<dyn Provider>> {
        Ok(Box::new(LaravelProvider::new(path, custom)?))
    }

    fn name(&self) -> &'static str {
        "laravel"
    }
}
