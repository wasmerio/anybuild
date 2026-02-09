//! Go provider for building Go applications.

use crate::providers::base::Provider;
use crate::providers::specs::{DependencySpec, DetectResult, ProviderPlan};
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Provider for Go applications.
pub struct GoProvider;

impl GoProvider {
    /// Parse go.mod to extract Go version.
    fn parse_go_version(path: &Path) -> Option<String> {
        let go_mod = path.join("go.mod");
        if !go_mod.exists() {
            return None;
        }

        let content = fs::read_to_string(&go_mod).ok()?;
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("go ") {
                let version = line.strip_prefix("go ")?.trim();
                return Some(version.to_string());
            }
        }

        None
    }

    /// Detect the main package location.
    fn detect_main_package(path: &Path) -> String {
        // Check common locations
        for candidate in &["cmd/server", "cmd/main", "cmd", "."] {
            let pkg_path = path.join(candidate);
            if pkg_path.exists() && pkg_path.is_dir() {
                return candidate.to_string();
            }
        }
        ".".to_string()
    }
}

impl Provider for GoProvider {
    fn name(&self) -> &str {
        "go"
    }

    fn priority(&self) -> i32 {
        5
    }

    fn detect(&self, path: &Path) -> Result<Option<DetectResult>> {
        let go_mod = path.join("go.mod");
        if !go_mod.exists() {
            return Ok(None);
        }

        let reason = "Found go.mod file".to_string();
        Ok(Some(DetectResult::new(self.name(), 1.0, reason)))
    }

    fn plan(&self, path: &Path) -> Result<ProviderPlan> {
        let mut plan = ProviderPlan::new("app", self.name());

        // Detect Go version
        let go_version =
            Self::parse_go_version(path).context("Could not detect Go version from go.mod")?;

        let mut go_dep = DependencySpec::new("go");
        go_dep.default_version = Some(go_version);
        go_dep.use_in_build = true;
        go_dep.use_in_serve = true;
        plan.dependencies.push(go_dep);

        // Detect main package
        let main_pkg = Self::detect_main_package(path);

        // Build steps
        plan.build_steps
            .push("run(\"go mod download\")".to_string());
        plan.build_steps
            .push(format!("run(\"go build -o app {}\")", main_pkg));

        // Serve command
        plan.commands.insert("web".to_string(), "./app".to_string());

        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_go_mod(path: &Path, content: &str) {
        fs::write(path.join("go.mod"), content).unwrap();
    }

    #[test]
    fn test_no_go_mod() {
        let tmp = TempDir::new().unwrap();
        let provider = GoProvider;

        let result = provider.detect(tmp.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_go_project() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        create_go_mod(
            path,
            r#"module example.com/myapp

go 1.21

require github.com/gin-gonic/gin v1.9.0
"#,
        );

        let provider = GoProvider;
        let result = provider.detect(path).unwrap().unwrap();

        assert_eq!(result.confidence, 1.0);
        assert!(result.reason.contains("go.mod"));
    }

    #[test]
    fn test_plan_generation() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        create_go_mod(
            path,
            r#"module example.com/myapp

go 1.21.5

require github.com/gin-gonic/gin v1.9.0
"#,
        );

        let provider = GoProvider;
        let plan = provider.plan(path).unwrap();

        assert_eq!(plan.serve_name, "app");
        assert!(plan.build_steps[0].contains("go mod download"));
        assert!(plan.build_steps[1].contains("go build"));
        assert!(plan.commands.get("web").unwrap().contains("./app"));

        // Should have Go dependency with version
        assert!(plan
            .dependencies
            .iter()
            .any(|d| d.name == "go" && d.default_version == Some("1.21.5".to_string())));
    }

    #[test]
    fn test_parse_go_version() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        create_go_mod(path, "module test\n\ngo 1.20\n");

        assert_eq!(GoProvider::parse_go_version(path), Some("1.20".to_string()));
    }

    #[test]
    fn test_main_package_detection() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        // Create cmd directory
        fs::create_dir(path.join("cmd")).unwrap();

        let main_pkg = GoProvider::detect_main_package(path);
        assert_eq!(main_pkg, "cmd");
    }
}
