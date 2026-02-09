//! Hugo provider for building Hugo static sites.

use crate::providers::base::Provider;
use crate::providers::specs::{DependencySpec, DetectResult, MountSpec, ProviderPlan};
use anyhow::Result;
use std::path::Path;

/// Provider for Hugo static site generator.
pub struct HugoProvider;

impl HugoProvider {
    /// Check if a Hugo config file exists.
    fn has_hugo_config(path: &Path) -> bool {
        path.join("hugo.toml").exists()
            || path.join("hugo.yaml").exists()
            || path.join("hugo.yml").exists()
            || path.join("config.toml").exists()
            || path.join("config.yaml").exists()
            || path.join("config.yml").exists()
    }
}

impl Provider for HugoProvider {
    fn name(&self) -> &str {
        "hugo"
    }

    fn priority(&self) -> i32 {
        8
    }

    fn detect(&self, path: &Path) -> Result<Option<DetectResult>> {
        if Self::has_hugo_config(path) {
            let reason = "Found Hugo configuration file".to_string();
            Ok(Some(DetectResult::new(self.name(), 1.0, reason)))
        } else {
            Ok(None)
        }
    }

    fn plan(&self, _path: &Path) -> Result<ProviderPlan> {
        let mut plan = ProviderPlan::new("public", self.name());

        // Hugo dependency
        let mut hugo_dep = DependencySpec::new("hugo");
        hugo_dep.use_in_build = true;
        plan.dependencies.push(hugo_dep);

        // Build step
        plan.build_steps.push("run(\"hugo\")".to_string());

        // Mount the public directory
        plan.mounts.push(MountSpec::new("public"));

        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_no_hugo_config() {
        let tmp = TempDir::new().unwrap();
        let provider = HugoProvider;

        let result = provider.detect(tmp.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_hugo_toml() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        fs::write(path.join("hugo.toml"), "baseURL = 'http://example.org/'").unwrap();

        let provider = HugoProvider;
        let result = provider.detect(path).unwrap().unwrap();

        assert_eq!(result.confidence, 1.0);
        assert!(result.reason.contains("Hugo configuration"));
    }

    #[test]
    fn test_detect_config_yaml() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        fs::write(path.join("config.yaml"), "baseURL: http://example.org/").unwrap();

        let provider = HugoProvider;
        let result = provider.detect(path).unwrap();

        assert!(result.is_some());
    }

    #[test]
    fn test_plan_generation() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        fs::write(path.join("hugo.toml"), "baseURL = 'http://example.org/'").unwrap();

        let provider = HugoProvider;
        let plan = provider.plan(path).unwrap();

        assert_eq!(plan.serve_name, "public");
        assert!(plan.build_steps[0].contains("hugo"));
        assert!(plan.dependencies.iter().any(|d| d.name == "hugo"));
        assert!(plan.mounts.iter().any(|m| m.name == "public"));
    }
}
