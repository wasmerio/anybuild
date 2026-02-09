//! Node.js static site generator detection.
//!
//! This module detects which static site generator or framework a Node.js
//! project uses and determines the output directory.

use serde_json::Value;
use std::collections::HashMap;

/// Supported static site generators and frameworks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaticGenerator {
    Astro,
    Vite,
    Next,
    Gatsby,
    Docusaurus,
    DocusaurusOld,
    Svelte,
    Remix,
    RemixV2,
    RemixV2Classic,
    NuxtOld,
    NuxtV3,
}

impl StaticGenerator {
    /// Returns the default output directory for this generator.
    pub fn get_output_dir(&self) -> &str {
        match self {
            Self::Next => "out",
            Self::NuxtV3 => ".output/public",
            Self::NuxtOld => "dist",
            Self::Gatsby => "public",
            Self::Remix => "build/client",
            Self::RemixV2 => "build/client",
            Self::RemixV2Classic => "public",
            Self::Docusaurus => "build",
            Self::DocusaurusOld => "build",
            Self::Astro => "dist",
            Self::Vite => "dist",
            Self::Svelte => "build",
        }
    }

    /// Returns the framework name.
    pub fn name(&self) -> &str {
        match self {
            Self::Astro => "Astro",
            Self::Vite => "Vite",
            Self::Next => "Next.js",
            Self::Gatsby => "Gatsby",
            Self::Docusaurus => "Docusaurus",
            Self::DocusaurusOld => "Docusaurus (old)",
            Self::Svelte => "SvelteKit",
            Self::Remix => "Remix",
            Self::RemixV2 => "Remix v2",
            Self::RemixV2Classic => "Remix v2 (classic)",
            Self::NuxtOld => "Nuxt (v2)",
            Self::NuxtV3 => "Nuxt (v3)",
        }
    }
}

/// Detects static site generators from package.json dependencies.
pub fn detect_from_dependencies(deps: &HashMap<String, Value>) -> Vec<StaticGenerator> {
    let mut generators = Vec::new();

    if deps.contains_key("astro") {
        generators.push(StaticGenerator::Astro);
    }
    if deps.contains_key("next") {
        generators.push(StaticGenerator::Next);
    }
    if deps.contains_key("gatsby") {
        generators.push(StaticGenerator::Gatsby);
    }
    if deps.contains_key("vite") {
        generators.push(StaticGenerator::Vite);
    }
    if deps.contains_key("@sveltejs/kit") {
        generators.push(StaticGenerator::Svelte);
    }

    // Remix detection
    if deps.contains_key("@remix-run/react") || deps.contains_key("@remix-run/node") {
        // Check version for Remix v2
        if let Some(version) = deps.get("@remix-run/react") {
            if let Some(ver_str) = version.as_str() {
                if ver_str.starts_with("2") || ver_str.starts_with("^2") {
                    generators.push(StaticGenerator::RemixV2);
                } else {
                    generators.push(StaticGenerator::Remix);
                }
            }
        } else {
            generators.push(StaticGenerator::Remix);
        }
    }

    // Nuxt detection
    if deps.contains_key("nuxt") {
        if let Some(version) = deps.get("nuxt") {
            if let Some(ver_str) = version.as_str() {
                if ver_str.starts_with("3") || ver_str.starts_with("^3") {
                    generators.push(StaticGenerator::NuxtV3);
                } else {
                    generators.push(StaticGenerator::NuxtOld);
                }
            } else {
                generators.push(StaticGenerator::NuxtV3);
            }
        } else {
            generators.push(StaticGenerator::NuxtV3);
        }
    }

    // Docusaurus detection
    if deps.contains_key("@docusaurus/core") {
        if let Some(version) = deps.get("@docusaurus/core") {
            if let Some(ver_str) = version.as_str() {
                if ver_str.starts_with("1") || ver_str.starts_with("^1") {
                    generators.push(StaticGenerator::DocusaurusOld);
                } else {
                    generators.push(StaticGenerator::Docusaurus);
                }
            } else {
                generators.push(StaticGenerator::Docusaurus);
            }
        } else {
            generators.push(StaticGenerator::Docusaurus);
        }
    }

    generators
}

/// Detects static site generators from build commands in package.json scripts.
pub fn detect_from_command(cmd: &str) -> Vec<StaticGenerator> {
    let mut generators = Vec::new();

    if cmd.contains("next build") {
        generators.push(StaticGenerator::Next);
    }
    if cmd.contains("astro build") {
        generators.push(StaticGenerator::Astro);
    }
    if cmd.contains("gatsby build") {
        generators.push(StaticGenerator::Gatsby);
    }
    if cmd.contains("vite build") {
        generators.push(StaticGenerator::Vite);
    }
    if cmd.contains("nuxt build") {
        generators.push(StaticGenerator::NuxtV3);
    }
    if cmd.contains("docusaurus build") {
        generators.push(StaticGenerator::Docusaurus);
    }
    if cmd.contains("svelte-kit build") {
        generators.push(StaticGenerator::Svelte);
    }
    if cmd.contains("remix build") {
        generators.push(StaticGenerator::Remix);
    }

    generators
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_output_directories() {
        assert_eq!(StaticGenerator::Next.get_output_dir(), "out");
        assert_eq!(StaticGenerator::NuxtV3.get_output_dir(), ".output/public");
        assert_eq!(StaticGenerator::Gatsby.get_output_dir(), "public");
        assert_eq!(StaticGenerator::Astro.get_output_dir(), "dist");
        assert_eq!(StaticGenerator::Vite.get_output_dir(), "dist");
        assert_eq!(StaticGenerator::Remix.get_output_dir(), "build/client");
    }

    #[test]
    fn test_framework_names() {
        assert_eq!(StaticGenerator::Next.name(), "Next.js");
        assert_eq!(StaticGenerator::Astro.name(), "Astro");
        assert_eq!(StaticGenerator::NuxtV3.name(), "Nuxt (v3)");
    }

    #[test]
    fn test_detect_astro() {
        let deps: HashMap<String, Value> = serde_json::from_value(json!({
            "astro": "^4.0.0"
        }))
        .unwrap();

        let generators = detect_from_dependencies(&deps);
        assert!(generators.contains(&StaticGenerator::Astro));
    }

    #[test]
    fn test_detect_next() {
        let deps: HashMap<String, Value> = serde_json::from_value(json!({
            "next": "14.0.0",
            "react": "^18.0.0"
        }))
        .unwrap();

        let generators = detect_from_dependencies(&deps);
        assert!(generators.contains(&StaticGenerator::Next));
    }

    #[test]
    fn test_detect_gatsby() {
        let deps: HashMap<String, Value> = serde_json::from_value(json!({
            "gatsby": "^5.0.0"
        }))
        .unwrap();

        let generators = detect_from_dependencies(&deps);
        assert!(generators.contains(&StaticGenerator::Gatsby));
    }

    #[test]
    fn test_detect_vite() {
        let deps: HashMap<String, Value> = serde_json::from_value(json!({
            "vite": "^5.0.0"
        }))
        .unwrap();

        let generators = detect_from_dependencies(&deps);
        assert!(generators.contains(&StaticGenerator::Vite));
    }

    #[test]
    fn test_detect_nuxt_v3() {
        let deps: HashMap<String, Value> = serde_json::from_value(json!({
            "nuxt": "^3.0.0"
        }))
        .unwrap();

        let generators = detect_from_dependencies(&deps);
        assert!(generators.contains(&StaticGenerator::NuxtV3));
    }

    #[test]
    fn test_detect_nuxt_old() {
        let deps: HashMap<String, Value> = serde_json::from_value(json!({
            "nuxt": "^2.15.0"
        }))
        .unwrap();

        let generators = detect_from_dependencies(&deps);
        assert!(generators.contains(&StaticGenerator::NuxtOld));
    }

    #[test]
    fn test_detect_remix_v2() {
        let deps: HashMap<String, Value> = serde_json::from_value(json!({
            "@remix-run/react": "^2.0.0",
            "@remix-run/node": "^2.0.0"
        }))
        .unwrap();

        let generators = detect_from_dependencies(&deps);
        assert!(generators.contains(&StaticGenerator::RemixV2));
    }

    #[test]
    fn test_detect_remix_v1() {
        let deps: HashMap<String, Value> = serde_json::from_value(json!({
            "@remix-run/react": "^1.19.0"
        }))
        .unwrap();

        let generators = detect_from_dependencies(&deps);
        assert!(generators.contains(&StaticGenerator::Remix));
    }

    #[test]
    fn test_detect_svelte() {
        let deps: HashMap<String, Value> = serde_json::from_value(json!({
            "@sveltejs/kit": "^2.0.0"
        }))
        .unwrap();

        let generators = detect_from_dependencies(&deps);
        assert!(generators.contains(&StaticGenerator::Svelte));
    }

    #[test]
    fn test_detect_docusaurus() {
        let deps: HashMap<String, Value> = serde_json::from_value(json!({
            "@docusaurus/core": "^3.0.0"
        }))
        .unwrap();

        let generators = detect_from_dependencies(&deps);
        assert!(generators.contains(&StaticGenerator::Docusaurus));
    }

    #[test]
    fn test_detect_from_build_command() {
        assert!(detect_from_command("next build").contains(&StaticGenerator::Next));
        assert!(detect_from_command("astro build").contains(&StaticGenerator::Astro));
        assert!(detect_from_command("gatsby build").contains(&StaticGenerator::Gatsby));
        assert!(detect_from_command("vite build").contains(&StaticGenerator::Vite));
        assert!(detect_from_command("nuxt build").contains(&StaticGenerator::NuxtV3));
    }

    #[test]
    fn test_detect_multiple_generators() {
        let deps: HashMap<String, Value> = serde_json::from_value(json!({
            "vite": "^5.0.0",
            "astro": "^4.0.0"
        }))
        .unwrap();

        let generators = detect_from_dependencies(&deps);
        assert!(generators.contains(&StaticGenerator::Vite));
        assert!(generators.contains(&StaticGenerator::Astro));
    }

    #[test]
    fn test_detect_no_generator() {
        let deps: HashMap<String, Value> = serde_json::from_value(json!({
            "express": "^4.0.0"
        }))
        .unwrap();

        let generators = detect_from_dependencies(&deps);
        assert!(generators.is_empty());
    }
}
