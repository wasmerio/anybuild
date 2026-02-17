//! Hugo provider for building Hugo static sites.

use crate::providers::base::Provider;
use crate::providers::specs::{DependencySpec, DetectResult, MountSpec, ProviderPlan};
use crate::starlark::config::ShipitConfig;
use anyhow::Result;
use std::fs;
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

    fn provider_config(&self, project_path: &Path) -> Result<ShipitConfig> {
        let mut config = ShipitConfig::new();

        // Detect Hugo version: look for old Hugo syntax
        let is_old_hugo = Self::detect_old_hugo(project_path);
        let hugo_version = if is_old_hugo {
            "0.139.0".to_string()
        } else {
            "0.153.2".to_string()
        };
        config.set("hugo_version", hugo_version);

        // Static output directory
        let static_dir =
            Self::detect_publish_dir(project_path).unwrap_or_else(|| "public".to_string());
        config.set("static_dir", static_dir);

        // sws version
        config.set("sws_version", "2.38.0");

        Ok(config)
    }
}

impl HugoProvider {
    /// Detect the publish directory from Hugo config.
    fn detect_publish_dir(path: &Path) -> Option<String> {
        let config_files = [
            "hugo.toml",
            "hugo.yaml",
            "hugo.yml",
            "config.toml",
            "config.yaml",
            "config.yml",
        ];
        for name in &config_files {
            let file_path = path.join(name);
            if !file_path.exists() {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&file_path) {
                // TOML: publishDir = "..."
                if name.ends_with(".toml") {
                    for line in content.lines() {
                        let line = line.trim();
                        if let Some(rest) = line.strip_prefix("publishDir") {
                            let rest = rest.trim();
                            if let Some(rest) = rest.strip_prefix('=') {
                                let val = rest.trim().trim_matches('"').trim_matches('\'');
                                if !val.is_empty() {
                                    return Some(val.to_string());
                                }
                            }
                        }
                    }
                } else {
                    // YAML: publishDir: ...
                    for line in content.lines() {
                        let line = line.trim();
                        if let Some(rest) = line.strip_prefix("publishDir:") {
                            let val = rest.trim().trim_matches('"').trim_matches('\'');
                            if !val.is_empty() {
                                return Some(val.to_string());
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Detect whether the project uses old Hugo syntax.
    fn detect_old_hugo(path: &Path) -> bool {
        // Check for resources.ToCSS usage in layouts
        let layouts = path.join("layouts");
        if layouts.exists() {
            if let Ok(entries) = fs::read_dir(&layouts) {
                for entry in entries.flatten() {
                    if let Ok(content) = fs::read_to_string(entry.path()) {
                        if content.contains("resources.ToCSS") {
                            return true;
                        }
                    }
                }
            }
        }
        false
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
