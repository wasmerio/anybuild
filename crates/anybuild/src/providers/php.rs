//! PHP provider implementation.
//!
//! PHP detection (composer.json / index.php probing), framework detection
//! (laravel/moodle/symfony/drupal incl. docroot resolution) and config
//! loading with the ANYBUILD_* env overlay.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::event::ProviderDetail;
use crate::operation::OperationContext;
use crate::providers::base::{env_bool, env_enum, env_int, env_str, BaseConfig, HasBase};
use crate::providers::{detection_details_from_fields, DetectableConfig};

pub const NAME: &str = "php";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PhpFramework {
    Laravel,
    Moodle,
    Symfony,
    Drupal,
}

impl PhpFramework {
    fn from_value(value: &str) -> Option<Self> {
        match value {
            "laravel" => Some(PhpFramework::Laravel),
            "moodle" => Some(PhpFramework::Moodle),
            "symfony" => Some(PhpFramework::Symfony),
            "drupal" => Some(PhpFramework::Drupal),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhpConfig {
    #[serde(flatten)]
    pub base: BaseConfig,
    pub framework: Option<PhpFramework>,
    pub phpix: bool,
    pub use_composer: bool,
    pub composer_build_script: Option<String>,
    pub php_version: Option<String>,
    /// `"64-bit"` or `"32-bit"`.
    pub php_architecture: Option<String>,
    pub phpix_worker_threads: Option<i64>,
    /// Docroot subdirectory ("web", "public", "app") or None for the root.
    pub public_dir: Option<String>,
}

impl Default for PhpConfig {
    fn default() -> Self {
        Self {
            base: BaseConfig::default(),
            framework: None,
            phpix: false,
            use_composer: false,
            composer_build_script: None,
            php_version: Some("8.3.29".to_owned()),
            php_architecture: None,
            phpix_worker_threads: Some(4),
            public_dir: None,
        }
    }
}

impl HasBase for PhpConfig {
    fn base(&self) -> &BaseConfig {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseConfig {
        &mut self.base
    }
}

impl PhpConfig {
    /// pydantic-settings construction: env applies to every field not
    /// passed as an init kwarg (the base fields always are).
    pub(crate) fn from_env(base: BaseConfig, operation: &OperationContext) -> Result<Self> {
        Ok(Self {
            base,
            framework: env_enum(
                operation,
                "framework",
                "a supported PHP framework",
                PhpFramework::from_value,
            )?,
            phpix: env_bool(operation, "phpix")?.unwrap_or(false),
            use_composer: env_bool(operation, "use_composer")?.unwrap_or(false),
            composer_build_script: env_str(operation, "composer_build_script"),
            php_version: env_str(operation, "php_version").or_else(|| Some("8.3.29".to_owned())),
            php_architecture: env_enum(
                operation,
                "php_architecture",
                "64-bit or 32-bit",
                |value| matches!(value, "64-bit" | "32-bit").then(|| value.to_owned()),
            )?,
            phpix_worker_threads: env_int(operation, "phpix_worker_threads")?.or(Some(4)),
            public_dir: env_str(operation, "public_dir"),
        })
    }
}

fn exists(path: &Path, candidates: &[&str]) -> bool {
    candidates.iter().any(|c| path.join(c).exists())
}

/// Port of `PhpProvider.load_composer_config`.
pub(crate) fn load_composer_config(path: &Path) -> Option<Map<String, Value>> {
    let contents = std::fs::read_to_string(path.join("composer.json")).ok()?;
    match serde_json::from_str::<Value>(&contents) {
        Ok(Value::Object(map)) => Some(map),
        _ => None,
    }
}

/// Port of `PhpProvider.composer_packages`.
fn composer_packages(composer_config: Option<&Map<String, Value>>) -> BTreeSet<String> {
    let mut packages = BTreeSet::new();
    let Some(config) = composer_config else {
        return packages;
    };
    for section in ["require", "require-dev"] {
        if let Some(Value::Object(deps)) = config.get(section) {
            packages.extend(deps.keys().map(|name| name.to_lowercase()));
        }
    }
    if let Some(Value::String(name)) = config.get("name") {
        packages.insert(name.to_lowercase());
    }
    packages
}

/// Port of `PhpProvider.detect_framework`.
pub(crate) fn detect_framework(
    path: &Path,
    composer_config: Option<&Map<String, Value>>,
) -> Option<PhpFramework> {
    let loaded;
    let composer_config = match composer_config {
        Some(config) => Some(config),
        None => {
            loaded = load_composer_config(path);
            loaded.as_ref()
        }
    };
    let composer_packages = composer_packages(composer_config);

    let has_moodle_layout = path.join("version.php").exists()
        && path.join("lib/setup.php").exists()
        && (path.join("admin/cli/install.php").exists()
            || path.join("mod").is_dir()
            || path.join("theme").is_dir());
    if has_moodle_layout {
        return Some(PhpFramework::Moodle);
    }

    let drupal_packages = [
        "drupal/core",
        "drupal/core-composer-scaffold",
        "drupal/core-recommended",
        "drupal/drupal",
        "drupal/recommended-project",
    ];
    let has_drupal_layout =
        path.join("core/lib/Drupal.php").exists() || path.join("web/core/lib/Drupal.php").exists();
    if has_drupal_layout
        || drupal_packages
            .iter()
            .any(|package| composer_packages.contains(*package))
    {
        return Some(PhpFramework::Drupal);
    }

    if path.join("artisan").exists() && path.join("composer.json").exists() {
        return Some(PhpFramework::Laravel);
    }

    if path.join("symfony.lock").exists()
        || composer_packages
            .iter()
            .any(|package| package.starts_with("symfony/"))
    {
        return Some(PhpFramework::Symfony);
    }

    None
}

/// Port of `PhpProvider.load_config`.
pub fn load_config(
    path: &Path,
    base: BaseConfig,
    operation: &OperationContext,
) -> Result<PhpConfig> {
    let composer_config = load_composer_config(path);
    let use_composer = exists(path, &["composer.json", "composer.lock"])
        || base
            .commands
            .install
            .as_deref()
            .is_some_and(|install| install.starts_with("composer "));
    let mut composer_build_script = None;
    if let Some(config) = &composer_config {
        if let Some(Value::Object(scripts)) = config.get("scripts") {
            composer_build_script = if scripts.contains_key("post-update-cmd") {
                Some("post-update-cmd".to_owned())
            } else if scripts.contains_key("post-install-cmd") {
                Some("post-install-cmd".to_owned())
            } else {
                None
            };
        }
    }

    let mut config = PhpConfig::from_env(base, operation)?;
    // use_composer / composer_build_script are init kwargs in Python, so
    // they never come from the env.
    config.use_composer = use_composer;
    config.composer_build_script = composer_build_script;

    if config.framework.is_none() {
        config.framework = detect_framework(path, composer_config.as_ref());
    }
    if config.framework == Some(PhpFramework::Drupal) {
        // Drupal relies on Apache-style rewrite behavior that the built-in
        // php server handles more predictably than phpix by default.
        config.phpix = false;
    }
    config.public_dir =
        if config.framework == Some(PhpFramework::Drupal) && exists(path, &["web/index.php"]) {
            Some("web".to_owned())
        } else if exists(path, &["public/index.php"]) {
            Some("public".to_owned())
        } else if exists(path, &["app/index.php"]) {
            Some("app".to_owned())
        } else {
            None
        };
    Ok(config)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DetectionEvidence {
    DrupalWeb,
    Framework,
    ComposerEntrypoint,
    Entrypoint,
    StartCommand,
    InstallCommand,
}

impl DetectableConfig for PhpConfig {
    type Evidence = DetectionEvidence;

    const DETECTION_DETAILS: &'static [(&'static str, &'static str)] =
        &[("Framework", "framework"), ("PHP version", "php_version")];

    fn detection_evidence(
        path: &Path,
        base: &BaseConfig,
        _operation: &OperationContext,
    ) -> Option<Self::Evidence> {
        let framework = detect_framework(path, None);
        if framework == Some(PhpFramework::Drupal) && path.join("web/index.php").exists() {
            return Some(DetectionEvidence::DrupalWeb);
        }
        if matches!(
            framework,
            Some(PhpFramework::Drupal | PhpFramework::Moodle | PhpFramework::Symfony)
        ) && exists(path, &["index.php", "public/index.php", "web/index.php"])
        {
            return Some(DetectionEvidence::Framework);
        }
        if path.join("composer.json").exists()
            && exists(path, &["public/index.php", "web/index.php"])
        {
            return Some(DetectionEvidence::ComposerEntrypoint);
        }
        if exists(
            path,
            &[
                "index.php",
                "public/index.php",
                "web/index.php",
                "app/index.php",
            ],
        ) {
            return Some(DetectionEvidence::Entrypoint);
        }
        if base
            .commands
            .start
            .as_deref()
            .is_some_and(|start| start.starts_with("php "))
        {
            return Some(DetectionEvidence::StartCommand);
        }
        if base
            .commands
            .install
            .as_deref()
            .is_some_and(|install| install.starts_with("composer "))
        {
            return Some(DetectionEvidence::InstallCommand);
        }
        None
    }

    fn load(path: &Path, base: BaseConfig, operation: &OperationContext) -> Result<Self> {
        load_config(path, base, operation)
    }

    fn detection_details(&self) -> Vec<ProviderDetail> {
        let mut details = detection_details_from_fields(self, Self::DETECTION_DETAILS);
        if self.use_composer {
            details.insert(
                usize::from(self.framework.is_some()),
                ProviderDetail {
                    label: "Package manager".to_owned(),
                    value: "Composer".to_owned(),
                },
            );
        }
        details
    }
}

#[cfg(test)]
mod tests {
    //! Port of `tests/test_php_provider.py`.

    use std::path::Path;

    use super::PhpFramework;
    use crate::providers::{
        load_provider_config_for_test as load_provider_config,
        load_provider_for_test as load_provider, BaseConfig, ProviderConfig,
    };

    fn write(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn load_php_config(project_dir: &Path) -> super::PhpConfig {
        let base = BaseConfig::default();
        let provider = load_provider(project_dir, &base, None).unwrap();
        assert_eq!(provider, "php");
        match load_provider_config(provider, project_dir, base).unwrap() {
            ProviderConfig::Php(config) => config,
            other => panic!("expected a php config, got {other:?}"),
        }
    }

    #[test]
    fn test_php_provider_detects_moodle_from_source() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("moodle");
        std::fs::create_dir_all(project_dir.join("admin/cli")).unwrap();
        std::fs::create_dir_all(project_dir.join("lib")).unwrap();
        std::fs::create_dir_all(project_dir.join("mod")).unwrap();
        write(&project_dir.join("index.php"), "<?php\n");
        write(&project_dir.join("version.php"), "<?php\n");
        write(&project_dir.join("lib/setup.php"), "<?php\n");
        write(&project_dir.join("admin/cli/install.php"), "<?php\n");

        let config = load_php_config(&project_dir);
        assert_eq!(config.framework, Some(PhpFramework::Moodle));
    }

    #[test]
    fn test_php_provider_detects_drupal_from_source() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("drupal");
        write(&project_dir.join("web/index.php"), "<?php\n");
        write(&project_dir.join("web/core/lib/Drupal.php"), "<?php\n");
        write(
            &project_dir.join("composer.json"),
            &serde_json::json!({
                "name": "drupal/recommended-project",
                "require": {"drupal/core-recommended": "^11.0"},
            })
            .to_string(),
        );

        let config = load_php_config(&project_dir);
        assert_eq!(config.framework, Some(PhpFramework::Drupal));
        assert_eq!(config.public_dir.as_deref(), Some("web"));
        // Drupal serves with plain php (phpix disabled) from the web/
        // docroot. The Python test asserts the evaluated start command
        // (`php -S localhost:… -t …/web`); the command rendering from
        // `phpix`/`public_dir` is pinned byte-for-byte by the php-nobuild
        // and php-api plan snapshots, so the config bits that drive it are
        // asserted here.
        assert!(!config.phpix);
    }

    #[test]
    fn test_php_provider_detects_drupal_from_source_layout() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("drupal-source");
        write(&project_dir.join("index.php"), "<?php\n");
        write(&project_dir.join("core/lib/Drupal.php"), "<?php\n");

        let config = load_php_config(&project_dir);
        assert_eq!(config.framework, Some(PhpFramework::Drupal));
    }
}
