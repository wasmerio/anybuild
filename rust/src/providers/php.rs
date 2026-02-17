//! PHP provider for building PHP applications.

use crate::providers::base::Provider;
use crate::providers::specs::{DependencySpec, DetectResult, ProviderPlan};
use crate::starlark::config::ShipitConfig;
use anyhow::Result;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// Provider for PHP applications.
pub struct PhpProvider;

impl PhpProvider {
    /// Check if composer.json exists.
    fn has_composer_json(path: &Path) -> bool {
        path.join("composer.json").exists()
    }

    /// Parse composer.json to detect frameworks.
    fn parse_composer_json(path: &Path) -> Option<Value> {
        let composer_file = path.join("composer.json");
        let content = fs::read_to_string(&composer_file).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Get all dependencies from composer.json.
    fn get_all_dependencies(composer_json: &Value) -> HashSet<String> {
        let mut deps = HashSet::new();

        if let Some(require) = composer_json.get("require") {
            if let Some(obj) = require.as_object() {
                for key in obj.keys() {
                    deps.insert(key.clone());
                }
            }
        }

        if let Some(require_dev) = composer_json.get("require-dev") {
            if let Some(obj) = require_dev.as_object() {
                for key in obj.keys() {
                    deps.insert(key.clone());
                }
            }
        }

        deps
    }

    /// Check if this is a Laravel application.
    fn is_laravel(deps: &HashSet<String>) -> bool {
        deps.contains("laravel/framework")
    }

    /// Detect PHP version from composer.json.
    fn detect_php_version(composer_json: &Value) -> Option<String> {
        let require = composer_json.get("require")?.as_object()?;
        let php_version = require.get("php")?.as_str()?;

        // Parse version constraint like "^8.1", ">=8.0", or ">= 8.2"
        let version = php_version
            .trim_start_matches('^')
            .trim_start_matches('~')
            .trim_start_matches(">=")
            .trim_start_matches('>')
            .trim()
            .split(['|', ' '])
            .find(|s| !s.is_empty())?;

        // Normalise to major.minor (e.g. "8.2.1" → "8.2")
        let parts: Vec<&str> = version.split('.').collect();
        let normalised = if parts.len() >= 2 {
            format!("{}.{}", parts[0], parts[1])
        } else {
            version.to_string()
        };

        Some(normalised)
    }
}

impl Provider for PhpProvider {
    fn name(&self) -> &str {
        "php"
    }

    fn priority(&self) -> i32 {
        5
    }

    fn detect(&self, path: &Path) -> Result<Option<DetectResult>> {
        // Check for index.php if no composer.json
        if !Self::has_composer_json(path) {
            if path.join("index.php").exists() {
                let reason = "Found index.php file".to_string();
                return Ok(Some(DetectResult::new(self.name(), 0.5, reason)));
            }
            return Ok(None);
        }

        // Parse composer.json
        let composer_json = match Self::parse_composer_json(path) {
            Some(json) => json,
            None => return Ok(None),
        };

        let deps = Self::get_all_dependencies(&composer_json);

        // Don't handle Laravel (separate provider)
        if Self::is_laravel(&deps) {
            return Ok(None);
        }

        let reason = "Found composer.json file".to_string();
        Ok(Some(DetectResult::new(self.name(), 0.8, reason)))
    }

    fn plan(&self, path: &Path) -> Result<ProviderPlan> {
        let mut plan = ProviderPlan::new("public", self.name());

        // PHP dependency
        let mut php_dep = DependencySpec::new("php");
        php_dep.use_in_build = true;
        php_dep.use_in_serve = true;

        // Try to detect PHP version
        if let Some(composer_json) = Self::parse_composer_json(path) {
            if let Some(version) = Self::detect_php_version(&composer_json) {
                php_dep.default_version = Some(version);
            }
        }

        plan.dependencies.push(php_dep);

        // Build steps (if composer.json exists)
        if Self::has_composer_json(path) {
            plan.build_steps
                .push("run(\"composer install --no-dev --optimize-autoloader\")".to_string());
        }

        // Serve command
        plan.commands.insert(
            "web".to_string(),
            "php -S 0.0.0.0:8080 -t public".to_string(),
        );

        Ok(plan)
    }

    fn provider_config(&self, project_path: &Path) -> Result<ShipitConfig> {
        let mut config = ShipitConfig::new();

        // PHP version from composer.json or default "8.3"
        let php_version = Self::parse_composer_json(project_path)
            .and_then(|json| Self::detect_php_version(&json))
            .unwrap_or_else(|| "8.3".to_string());
        config.set("php_version", php_version);
        config.set("php_architecture", "64-bit");

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_no_php_files() {
        let tmp = TempDir::new().unwrap();
        let provider = PhpProvider;

        let result = provider.detect(tmp.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_index_php() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        fs::write(path.join("index.php"), "<?php echo 'Hello'; ?>").unwrap();

        let provider = PhpProvider;
        let result = provider.detect(path).unwrap().unwrap();

        assert_eq!(result.confidence, 0.5);
        assert!(result.reason.contains("index.php"));
    }

    #[test]
    fn test_detect_composer_json() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        fs::write(
            path.join("composer.json"),
            r#"{"require": {"php": "^8.1"}}"#,
        )
        .unwrap();

        let provider = PhpProvider;
        let result = provider.detect(path).unwrap().unwrap();

        assert_eq!(result.confidence, 0.8);
        assert!(result.reason.contains("composer.json"));
    }

    #[test]
    fn test_skip_laravel() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        fs::write(
            path.join("composer.json"),
            r#"{"require": {"laravel/framework": "^10.0"}}"#,
        )
        .unwrap();

        let provider = PhpProvider;
        let result = provider.detect(path).unwrap();

        // Should not detect Laravel apps
        assert!(result.is_none());
    }

    #[test]
    fn test_plan_with_composer() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        fs::write(
            path.join("composer.json"),
            r#"{"require": {"php": "^8.2"}}"#,
        )
        .unwrap();

        let provider = PhpProvider;
        let plan = provider.plan(path).unwrap();

        assert_eq!(plan.serve_name, "public");
        assert!(plan.build_steps[0].contains("composer install"));
        assert!(plan.commands.get("web").unwrap().contains("php -S"));
        assert!(plan
            .dependencies
            .iter()
            .any(|d| d.name == "php" && d.default_version == Some("8.2".to_string())));
    }

    #[test]
    fn test_plan_without_composer() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        fs::write(path.join("index.php"), "<?php echo 'Hello'; ?>").unwrap();

        let provider = PhpProvider;
        let plan = provider.plan(path).unwrap();

        // No build steps without composer
        assert!(plan.build_steps.is_empty());
        assert!(plan.commands.get("web").is_some());
    }

    #[test]
    fn test_detect_php_version() {
        let json: Value = serde_json::from_str(r#"{"require": {"php": "^8.1"}}"#).unwrap();
        assert_eq!(
            PhpProvider::detect_php_version(&json),
            Some("8.1".to_string())
        );

        let json: Value = serde_json::from_str(r#"{"require": {"php": ">=8.0"}}"#).unwrap();
        assert_eq!(
            PhpProvider::detect_php_version(&json),
            Some("8.0".to_string())
        );
    }
}
