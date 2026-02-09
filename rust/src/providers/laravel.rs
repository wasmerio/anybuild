//! Laravel provider for building Laravel applications.

use crate::providers::base::Provider;
use crate::providers::specs::{DependencySpec, DetectResult, ProviderPlan};
use anyhow::Result;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// Provider for Laravel applications.
pub struct LaravelProvider;

impl LaravelProvider {
    /// Parse composer.json.
    fn parse_composer_json(path: &Path) -> Option<Value> {
        let composer_file = path.join("composer.json");
        let content = fs::read_to_string(&composer_file).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Get all dependencies.
    fn get_all_dependencies(composer_json: &Value) -> HashSet<String> {
        let mut deps = HashSet::new();

        if let Some(require) = composer_json.get("require") {
            if let Some(obj) = require.as_object() {
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

        let version = php_version
            .trim_start_matches('^')
            .trim_start_matches('~')
            .trim_start_matches(">=")
            .trim_start_matches('>')
            .split(['|', ' '])
            .next()?;

        Some(version.to_string())
    }

    /// Check if package.json exists (Laravel with Vite/Mix).
    fn has_package_json(path: &Path) -> bool {
        path.join("package.json").exists()
    }

    /// Parse package.json to check for build tools.
    fn parse_package_json(path: &Path) -> Option<Value> {
        let pkg_file = path.join("package.json");
        let content = fs::read_to_string(&pkg_file).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Check if project uses Vite (Laravel 9+).
    fn uses_vite(path: &Path) -> bool {
        if let Some(pkg_json) = Self::parse_package_json(path) {
            if let Some(deps) = pkg_json.get("devDependencies") {
                if let Some(obj) = deps.as_object() {
                    return obj.contains_key("vite") || obj.contains_key("laravel-vite-plugin");
                }
            }
        }
        false
    }
}

impl Provider for LaravelProvider {
    fn name(&self) -> &str {
        "laravel"
    }

    fn priority(&self) -> i32 {
        10 // Higher priority than generic PHP
    }

    fn detect(&self, path: &Path) -> Result<Option<DetectResult>> {
        let composer_json = match Self::parse_composer_json(path) {
            Some(json) => json,
            None => return Ok(None),
        };

        let deps = Self::get_all_dependencies(&composer_json);

        if !Self::is_laravel(&deps) {
            return Ok(None);
        }

        let reason = "Found laravel/framework in composer.json".to_string();
        Ok(Some(DetectResult::new(self.name(), 1.0, reason)))
    }

    fn plan(&self, path: &Path) -> Result<ProviderPlan> {
        let mut plan = ProviderPlan::new("public", self.name());

        // PHP dependency
        let mut php_dep = DependencySpec::new("php");
        php_dep.use_in_build = true;
        php_dep.use_in_serve = true;

        if let Some(composer_json) = Self::parse_composer_json(path) {
            if let Some(version) = Self::detect_php_version(&composer_json) {
                php_dep.default_version = Some(version);
            }
        }

        plan.dependencies.push(php_dep);

        // Composer install
        plan.build_steps
            .push("run(\"composer install --no-dev --optimize-autoloader\")".to_string());

        // Check if we need to build frontend assets
        if Self::has_package_json(path) {
            // Node dependency for building assets
            let mut node_dep = DependencySpec::new("node");
            node_dep.use_in_build = true;
            plan.dependencies.push(node_dep);

            plan.build_steps.push("run(\"npm install\")".to_string());

            if Self::uses_vite(path) {
                plan.build_steps.push("run(\"npm run build\")".to_string());
            } else {
                // Laravel Mix
                plan.build_steps
                    .push("run(\"npm run production\")".to_string());
            }
        }

        // Artisan commands for optimization
        plan.build_steps
            .push("run(\"php artisan config:cache\")".to_string());
        plan.build_steps
            .push("run(\"php artisan route:cache\")".to_string());
        plan.build_steps
            .push("run(\"php artisan view:cache\")".to_string());

        // Serve command
        plan.commands.insert(
            "web".to_string(),
            "php artisan serve --host=0.0.0.0 --port=8080".to_string(),
        );

        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_no_laravel() {
        let tmp = TempDir::new().unwrap();
        let provider = LaravelProvider;

        let result = provider.detect(tmp.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_laravel() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        fs::write(
            path.join("composer.json"),
            r#"{"require": {"php": "^8.1", "laravel/framework": "^10.0"}}"#,
        )
        .unwrap();

        let provider = LaravelProvider;
        let result = provider.detect(path).unwrap().unwrap();

        assert_eq!(result.confidence, 1.0);
        assert!(result.reason.contains("laravel/framework"));
    }

    #[test]
    fn test_plan_without_frontend() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        fs::write(
            path.join("composer.json"),
            r#"{"require": {"php": "^8.2", "laravel/framework": "^10.0"}}"#,
        )
        .unwrap();

        let provider = LaravelProvider;
        let plan = provider.plan(path).unwrap();

        assert!(plan.build_steps[0].contains("composer install"));
        assert!(plan.build_steps.iter().any(|s| s.contains("config:cache")));
        assert!(plan.commands.get("web").unwrap().contains("artisan serve"));

        // Should not have Node dependency
        assert!(!plan.dependencies.iter().any(|d| d.name == "node"));
    }

    #[test]
    fn test_plan_with_vite() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        fs::write(
            path.join("composer.json"),
            r#"{"require": {"php": "^8.2", "laravel/framework": "^10.0"}}"#,
        )
        .unwrap();

        fs::write(
            path.join("package.json"),
            r#"{"devDependencies": {"vite": "^4.0", "laravel-vite-plugin": "^0.8"}}"#,
        )
        .unwrap();

        let provider = LaravelProvider;
        let plan = provider.plan(path).unwrap();

        assert_eq!(plan.serve_name, "public");
        assert!(plan.dependencies.iter().any(|d| d.name == "node"));
        assert!(plan.build_steps.iter().any(|s| s.contains("npm install")));
        assert!(plan.build_steps.iter().any(|s| s.contains("npm run build")));
    }

    #[test]
    fn test_plan_with_mix() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        fs::write(
            path.join("composer.json"),
            r#"{"require": {"php": "^8.1", "laravel/framework": "^9.0"}}"#,
        )
        .unwrap();

        fs::write(
            path.join("package.json"),
            r#"{"devDependencies": {"laravel-mix": "^6.0"}}"#,
        )
        .unwrap();

        let provider = LaravelProvider;
        let plan = provider.plan(path).unwrap();

        assert!(plan.dependencies.iter().any(|d| d.name == "node"));
        assert!(plan
            .build_steps
            .iter()
            .any(|s| s.contains("npm run production")));
    }

    #[test]
    fn test_priority_higher_than_php() {
        let provider = LaravelProvider;
        assert!(provider.priority() > 5); // Higher than PhpProvider
    }
}
