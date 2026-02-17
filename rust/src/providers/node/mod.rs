//! Node.js static site provider.
//!
//! Detects and builds Node.js-based static sites using frameworks like
//! Next.js, Nuxt, Gatsby, Astro, etc.

mod frameworks;
mod package_manager;

use crate::providers::base::Provider;
use crate::providers::specs::{DependencySpec, DetectResult, MountSpec, ProviderPlan};
use crate::starlark::config::ShipitConfig;
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub use frameworks::StaticGenerator;
pub use package_manager::PackageManager;

/// Provider for Node.js static sites.
pub struct NodeStaticProvider;

impl NodeStaticProvider {
    /// Parse package.json from a directory.
    fn parse_package_json(path: &Path) -> Result<Value> {
        let pkg_path = path.join("package.json");
        let content = fs::read_to_string(&pkg_path).context("Failed to read package.json")?;
        serde_json::from_str(&content).context("Failed to parse package.json")
    }

    /// Get all dependencies from package.json.
    fn get_all_dependencies(pkg: &Value) -> HashMap<String, Value> {
        let mut all_deps = HashMap::new();

        if let Some(deps) = pkg.get("dependencies").and_then(|d| d.as_object()) {
            for (k, v) in deps {
                all_deps.insert(k.clone(), v.clone());
            }
        }

        if let Some(deps) = pkg.get("devDependencies").and_then(|d| d.as_object()) {
            for (k, v) in deps {
                all_deps.insert(k.clone(), v.clone());
            }
        }

        all_deps
    }

    /// Get the build script from package.json.
    fn get_build_script(pkg: &Value) -> Option<String> {
        pkg.get("scripts")
            .and_then(|s| s.get("build"))
            .and_then(|b| b.as_str())
            .map(|s| s.to_string())
    }

    /// Detect Node.js version from engines field.
    fn detect_node_version(pkg: &Value) -> Option<String> {
        Self::detect_node_version_from_pkg(pkg)
    }

    /// Public helper to detect Node.js version from a package.json
    /// Value. Used by other providers (e.g. Laravel).
    pub fn detect_node_version_from_pkg(pkg: &Value) -> Option<String> {
        pkg.get("engines")
            .and_then(|e| e.get("node"))
            .and_then(|n| n.as_str())
            .map(|s| {
                // Strip prefixes like ^, ~, >= and extract version
                s.trim_start_matches('^')
                    .trim_start_matches('~')
                    .trim_start_matches(">=")
                    .trim()
                    .to_string()
            })
    }

    /// Detect the output directory for the project.
    fn detect_output_dir(path: &Path, generators: &[StaticGenerator]) -> String {
        // If we detected a generator, use its default output dir
        if let Some(gen) = generators.first() {
            return gen.get_output_dir().to_string();
        }

        // Check common output directories
        for dir in &["dist", "build", "out", "public", ".output/public"] {
            if path.join(dir).exists() {
                return dir.to_string();
            }
        }

        // Default to dist
        "dist".to_string()
    }
}

impl Provider for NodeStaticProvider {
    fn name(&self) -> &str {
        "node-static"
    }

    fn priority(&self) -> i32 {
        5
    }

    fn detect(&self, path: &Path) -> Result<Option<DetectResult>> {
        // Check for package.json
        let pkg_path = path.join("package.json");
        if !pkg_path.exists() {
            return Ok(None);
        }

        // Parse package.json
        let pkg = Self::parse_package_json(path)?;
        let all_deps = Self::get_all_dependencies(&pkg);

        // Detect static site generators
        let mut generators = frameworks::detect_from_dependencies(&all_deps);

        // Also check build script
        if let Some(build_script) = Self::get_build_script(&pkg) {
            let from_cmd = frameworks::detect_from_command(&build_script);
            for gen in from_cmd {
                if !generators.contains(&gen) {
                    generators.push(gen);
                }
            }
        }

        // Calculate confidence
        let confidence = if !generators.is_empty() {
            0.9
        } else if Self::get_build_script(&pkg).is_some() {
            0.7
        } else {
            0.5
        };

        // Build reason
        let reason = if let Some(gen) = generators.first() {
            format!("Detected {} project", gen.name())
        } else {
            "Found package.json with build script".to_string()
        };

        Ok(Some(DetectResult::new(self.name(), confidence, reason)))
    }

    fn plan(&self, path: &Path) -> Result<ProviderPlan> {
        // Parse package.json
        let pkg = Self::parse_package_json(path)?;
        let all_deps = Self::get_all_dependencies(&pkg);

        // Detect package manager
        let pm = package_manager::detect_package_manager(path).unwrap_or(PackageManager::Npm);
        let has_lockfile = path.join(pm.lockfile()).exists();

        // Detect generators
        let generators = frameworks::detect_from_dependencies(&all_deps);

        // Always use "build" as the script key
        let build_script_key = "build";

        // Detect output directory
        let output_dir = Self::detect_output_dir(path, &generators);

        // Create plan
        let mut plan = ProviderPlan::new("app", self.name());

        // Build steps (as Starlark commands)
        plan.build_steps
            .push(format!("run(\"{}\")", pm.install_command(has_lockfile)));
        plan.build_steps
            .push(format!("run(\"{}\")", pm.run_command(build_script_key)));

        // Dependencies
        plan.dependencies.push(pm.as_dependency());

        // Add Node.js version if specified
        if let Some(node_version) = Self::detect_node_version(&pkg) {
            let mut node_dep = DependencySpec::new("node");
            node_dep.default_version = Some(node_version);
            node_dep.use_in_build = true;
            plan.dependencies.push(node_dep);
        }

        // Add pnpm version if using pnpm
        if pm == PackageManager::Pnpm {
            let lockfile_path = path.join("pnpm-lock.yaml");
            if let Some(pnpm_version) = package_manager::detect_pnpm_version(&lockfile_path) {
                let mut pnpm_dep = DependencySpec::new("pnpm");
                pnpm_dep.default_version = Some(pnpm_version);
                pnpm_dep.use_in_build = true;
                plan.dependencies.push(pnpm_dep);
            }
        }

        // Mount output directory
        plan.mounts.push(MountSpec::new(output_dir));

        Ok(plan)
    }

    fn provider_config(&self, project_path: &Path) -> Result<ShipitConfig> {
        let mut config = ShipitConfig::new();

        // Node version from package.json engines or default
        let node_version = Self::parse_package_json(project_path)
            .ok()
            .and_then(|pkg| Self::detect_node_version(&pkg))
            .unwrap_or_else(|| "22".to_string());
        config.set("node_version", node_version);

        // npm version
        config.set_option("npm_version", None::<String>);

        // sws version for serving
        config.set("sws_version", "2.38.0");

        // static_dir from generator detection
        let generators = Self::parse_package_json(project_path)
            .ok()
            .map(|pkg| {
                let all_deps = Self::get_all_dependencies(&pkg);
                frameworks::detect_from_dependencies(&all_deps)
            })
            .unwrap_or_default();
        let static_dir = Self::detect_output_dir(project_path, &generators);
        config.set("static_dir", static_dir);

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_package_json(path: &Path, content: &str) {
        fs::write(path.join("package.json"), content).unwrap();
    }

    #[test]
    fn test_no_package_json() {
        let tmp = TempDir::new().unwrap();
        let provider = NodeStaticProvider;

        let result = provider.detect(tmp.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_next_project() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        create_package_json(
            path,
            r#"{
                "dependencies": {
                    "next": "14.0.0",
                    "react": "^18.0.0"
                },
                "scripts": {
                    "build": "next build"
                }
            }"#,
        );

        let provider = NodeStaticProvider;
        let result = provider.detect(path).unwrap().unwrap();

        assert_eq!(result.confidence, 0.9);
        assert!(result.reason.contains("Next.js"));
    }

    #[test]
    fn test_detect_astro_project() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        create_package_json(
            path,
            r#"{
                "dependencies": {
                    "astro": "^4.0.0"
                },
                "scripts": {
                    "build": "astro build"
                }
            }"#,
        );

        let provider = NodeStaticProvider;
        let result = provider.detect(path).unwrap().unwrap();

        assert_eq!(result.confidence, 0.9);
        assert!(result.reason.contains("Astro"));
    }

    #[test]
    fn test_detect_with_pnpm() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        create_package_json(
            path,
            r#"{
                "dependencies": {
                    "vite": "^5.0.0"
                }
            }"#,
        );
        fs::write(path.join("pnpm-lock.yaml"), "lockfileVersion: 6.0\n").unwrap();

        let provider = NodeStaticProvider;
        let result = provider.detect(path).unwrap();

        assert!(result.is_some());
    }

    #[test]
    fn test_detect_with_build_script() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        create_package_json(
            path,
            r#"{
                "scripts": {
                    "build": "vite build"
                }
            }"#,
        );

        let provider = NodeStaticProvider;
        let result = provider.detect(path).unwrap().unwrap();

        // Vite is detected from command, so confidence should be 0.9
        assert_eq!(result.confidence, 0.9);
    }

    #[test]
    fn test_detect_no_build_script() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        create_package_json(
            path,
            r#"{
                "dependencies": {
                    "express": "^4.0.0"
                }
            }"#,
        );

        let provider = NodeStaticProvider;
        let result = provider.detect(path).unwrap().unwrap();

        assert_eq!(result.confidence, 0.5);
    }

    #[test]
    fn test_plan_generation() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        create_package_json(
            path,
            r#"{
                "dependencies": {
                    "next": "14.0.0"
                },
                "scripts": {
                    "build": "next build"
                },
                "engines": {
                    "node": ">=20.0.0"
                }
            }"#,
        );
        fs::write(path.join("package-lock.json"), "").unwrap();

        let provider = NodeStaticProvider;
        let plan = provider.plan(path).unwrap();

        assert_eq!(plan.build_steps.len(), 2);
        assert!(plan.build_steps[0].contains("npm ci"));
        assert!(plan.build_steps[1].contains("npm run build"));

        // Should have Node.js dependency with version
        assert!(plan
            .dependencies
            .iter()
            .any(|d| d.name == "node" && d.default_version.is_some()));

        // Should mount output directory
        assert_eq!(plan.mounts.len(), 1);
    }

    #[test]
    fn test_plan_with_yarn() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        create_package_json(
            path,
            r#"{
                "dependencies": {
                    "gatsby": "^5.0.0"
                },
                "scripts": {
                    "build": "gatsby build"
                }
            }"#,
        );
        fs::write(path.join("yarn.lock"), "").unwrap();

        let provider = NodeStaticProvider;
        let plan = provider.plan(path).unwrap();

        // Should use yarn commands
        assert!(plan.build_steps[0].contains("yarn"));

        // Output should be "public" for Gatsby
        assert_eq!(plan.mounts[0].name, "public");
    }

    #[test]
    fn test_plan_with_bun() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        create_package_json(
            path,
            r#"{
                "dependencies": {
                    "vite": "^5.0.0"
                }
            }"#,
        );
        fs::write(path.join("bun.lockb"), "").unwrap();

        let provider = NodeStaticProvider;
        let plan = provider.plan(path).unwrap();

        // Should use bun commands
        assert!(plan.build_steps[0].contains("bun"));

        // Should have bun dependency
        assert!(plan.dependencies.iter().any(|d| d.name == "bun"));
    }

    #[test]
    fn test_output_dir_detection() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        create_package_json(
            path,
            r#"{
                "dependencies": {
                    "next": "14.0.0"
                }
            }"#,
        );

        let provider = NodeStaticProvider;
        let plan = provider.plan(path).unwrap();

        // Next.js uses "out"
        assert_eq!(plan.mounts[0].name, "out");
    }
}
