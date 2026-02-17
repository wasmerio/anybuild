//! Staticfile provider - fallback for serving static files

use crate::providers::{base, DetectResult, Provider, ProviderPlan};
use crate::starlark::config::ShipitConfig;
use anyhow::Result;
use std::path::Path;

/// Staticfile provider serves static content
///
/// This is a fallback provider with low priority that always matches.
/// It's used when no other provider can be detected.
pub struct StaticfileProvider;

impl Provider for StaticfileProvider {
    fn name(&self) -> &str {
        "staticfile"
    }

    fn detect(&self, path: &Path) -> Result<Option<DetectResult>> {
        // Always detect but with low confidence
        // Check if there are any common static files
        let has_html = base::detect_by_pattern(path, "*.html");
        let has_index = base::detect_by_file(path, "index.html");
        let has_css = base::detect_by_pattern(path, "*.css");
        let has_js = base::detect_by_pattern(path, "*.js");

        let confidence = if has_index {
            0.5 // Higher confidence if index.html exists
        } else if has_html || has_css || has_js {
            0.4 // Medium-low confidence if static files exist
        } else {
            0.2 // Very low confidence as complete fallback
        };

        let reason = if has_index {
            "Found index.html"
        } else if has_html {
            "Found HTML files"
        } else {
            "Fallback provider for static content"
        };

        Ok(Some(DetectResult::new(self.name(), confidence, reason)))
    }

    fn plan(&self, _path: &Path) -> Result<ProviderPlan> {
        let mut plan = ProviderPlan::new("static-app", "staticfile");

        // Add build steps to copy everything to output
        plan.build_steps.push(r#"copy(".", ".")"#.to_string());

        // Add serve command using a simple HTTP server
        plan.commands.insert(
            "web".to_string(),
            "python3 -m http.server $PORT".to_string(),
        );

        // Add Python as a dependency for the HTTP server
        let mut python_dep = crate::providers::DependencySpec::new("python");
        python_dep.use_in_serve = true;
        python_dep.default_version = Some("3.11".to_string());
        plan.dependencies.push(python_dep);

        Ok(plan)
    }

    fn priority(&self) -> i32 {
        -100 // Very low priority - used as fallback
    }

    fn provider_config(&self, _project_path: &Path) -> Result<ShipitConfig> {
        let mut config = ShipitConfig::new();
        config.set("sws_version", "2.38.0");
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_staticfile_name() {
        let provider = StaticfileProvider;
        assert_eq!(provider.name(), "staticfile");
    }

    #[test]
    fn test_staticfile_priority() {
        let provider = StaticfileProvider;
        assert_eq!(provider.priority(), -100);
    }

    #[test]
    fn test_detect_with_index_html() {
        let temp = TempDir::new().unwrap();
        let path = temp.path();

        fs::write(path.join("index.html"), "<html></html>").unwrap();

        let provider = StaticfileProvider;
        let result = provider.detect(path).unwrap();

        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.name, "staticfile");
        assert_eq!(result.confidence, 0.5);
        assert!(result.reason.contains("index.html"));
    }

    #[test]
    fn test_detect_with_html_files() {
        let temp = TempDir::new().unwrap();
        let path = temp.path();

        fs::write(path.join("about.html"), "<html></html>").unwrap();

        let provider = StaticfileProvider;
        let result = provider.detect(path).unwrap();

        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.confidence, 0.4);
    }

    #[test]
    fn test_detect_fallback() {
        let temp = TempDir::new().unwrap();
        let path = temp.path();

        // Empty directory
        let provider = StaticfileProvider;
        let result = provider.detect(path).unwrap();

        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.confidence, 0.2);
        assert!(result.reason.contains("Fallback"));
    }

    #[test]
    fn test_plan_generation() {
        let temp = TempDir::new().unwrap();
        let path = temp.path();

        let provider = StaticfileProvider;
        let plan = provider.plan(path).unwrap();

        assert_eq!(plan.serve_name, "static-app");
        assert_eq!(plan.provider, "staticfile");
        assert_eq!(plan.build_steps.len(), 1);
        assert!(plan.build_steps[0].contains("copy"));
        assert!(plan.commands.contains_key("web"));
        assert_eq!(plan.dependencies.len(), 1);
        assert_eq!(plan.dependencies[0].name, "python");
    }
}
