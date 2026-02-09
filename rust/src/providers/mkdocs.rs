//! MkDocs provider for building MkDocs documentation sites.

use crate::providers::base::Provider;
use crate::providers::specs::{DependencySpec, DetectResult, MountSpec, ProviderPlan};
use anyhow::Result;
use std::fs;
use std::path::Path;

/// Provider for MkDocs documentation sites.
pub struct MkDocsProvider;

impl MkDocsProvider {
    /// Check if MkDocs config exists.
    fn has_mkdocs_config(path: &Path) -> bool {
        path.join("mkdocs.yml").exists() || path.join("mkdocs.yaml").exists()
    }

    /// Parse requirements.txt to check for plugins.
    fn has_requirements(path: &Path) -> bool {
        path.join("requirements.txt").exists()
    }

    /// Parse requirements.txt and extract packages.
    #[allow(dead_code)]
    fn parse_requirements(path: &Path) -> Vec<String> {
        let req_file = path.join("requirements.txt");
        if !req_file.exists() {
            return vec![];
        }

        let content = match fs::read_to_string(&req_file) {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        content
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    None
                } else {
                    // Extract package name before version specifier
                    let pkg = line
                        .split(&['=', '<', '>', '!', '~'][..])
                        .next()
                        .unwrap_or(line)
                        .trim();
                    Some(pkg.to_string())
                }
            })
            .collect()
    }
}

impl Provider for MkDocsProvider {
    fn name(&self) -> &str {
        "mkdocs"
    }

    fn priority(&self) -> i32 {
        8
    }

    fn detect(&self, path: &Path) -> Result<Option<DetectResult>> {
        if Self::has_mkdocs_config(path) {
            let reason = "Found mkdocs.yml configuration file".to_string();
            Ok(Some(DetectResult::new(self.name(), 1.0, reason)))
        } else {
            Ok(None)
        }
    }

    fn plan(&self, path: &Path) -> Result<ProviderPlan> {
        let mut plan = ProviderPlan::new("site", self.name());

        // Python dependency
        let mut python_dep = DependencySpec::new("python");
        python_dep.use_in_build = true;
        plan.dependencies.push(python_dep);

        // Build steps
        if Self::has_requirements(path) {
            plan.build_steps
                .push("run(\"pip install -r requirements.txt\")".to_string());
        } else {
            plan.build_steps
                .push("run(\"pip install mkdocs\")".to_string());
        }

        plan.build_steps.push("run(\"mkdocs build\")".to_string());

        // Mount the site directory
        plan.mounts.push(MountSpec::new("site"));

        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_no_mkdocs_config() {
        let tmp = TempDir::new().unwrap();
        let provider = MkDocsProvider;

        let result = provider.detect(tmp.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_mkdocs_yml() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        fs::write(
            path.join("mkdocs.yml"),
            "site_name: My Docs\ntheme:\n  name: material",
        )
        .unwrap();

        let provider = MkDocsProvider;
        let result = provider.detect(path).unwrap().unwrap();

        assert_eq!(result.confidence, 1.0);
        assert!(result.reason.contains("mkdocs.yml"));
    }

    #[test]
    fn test_detect_mkdocs_yaml() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        fs::write(path.join("mkdocs.yaml"), "site_name: My Docs").unwrap();

        let provider = MkDocsProvider;
        let result = provider.detect(path).unwrap();

        assert!(result.is_some());
    }

    #[test]
    fn test_plan_without_requirements() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        fs::write(path.join("mkdocs.yml"), "site_name: My Docs").unwrap();

        let provider = MkDocsProvider;
        let plan = provider.plan(path).unwrap();

        assert_eq!(plan.serve_name, "site");
        assert!(plan.build_steps[0].contains("pip install mkdocs"));
        assert!(plan.build_steps[1].contains("mkdocs build"));
        assert!(plan.dependencies.iter().any(|d| d.name == "python"));
    }

    #[test]
    fn test_plan_with_requirements() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        fs::write(path.join("mkdocs.yml"), "site_name: My Docs").unwrap();
        fs::write(
            path.join("requirements.txt"),
            "mkdocs==1.5.0\nmkdocs-material==9.0.0",
        )
        .unwrap();

        let provider = MkDocsProvider;
        let plan = provider.plan(path).unwrap();

        assert!(plan.build_steps[0].contains("pip install -r requirements.txt"));
    }

    #[test]
    fn test_parse_requirements() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        fs::write(
            path.join("requirements.txt"),
            "mkdocs==1.5.0\n# comment\nmkdocs-material>=9.0.0\n\npymdown-extensions~=10.0",
        )
        .unwrap();

        let packages = MkDocsProvider::parse_requirements(path);

        assert_eq!(packages.len(), 3);
        assert!(packages.contains(&"mkdocs".to_string()));
        assert!(packages.contains(&"mkdocs-material".to_string()));
        assert!(packages.contains(&"pymdown-extensions".to_string()));
    }
}
