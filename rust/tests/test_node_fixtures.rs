//! Integration tests for provider detection using real project fixtures.

use shipit::providers::{base::Provider, node::NodeStaticProvider};
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn test_detect_nextjs_fixture() {
    let path = fixture_path("node-nextjs");
    let provider = NodeStaticProvider;

    let result = provider.detect(&path).unwrap().unwrap();
    assert_eq!(result.name, "node-static");
    assert_eq!(result.confidence, 0.9);
    assert!(result.reason.contains("Next.js"));
}

#[test]
fn test_plan_nextjs_fixture() {
    let path = fixture_path("node-nextjs");
    let provider = NodeStaticProvider;

    let plan = provider.plan(&path).unwrap();

    // Should use npm (has package-lock.json)
    assert!(plan.build_steps[0].contains("npm ci"));
    assert!(plan.build_steps[1].contains("npm run build"));

    // Should detect Node.js version
    assert!(plan.dependencies.iter().any(|d| {
        d.name == "node"
            && d.default_version
                .as_ref()
                .map(|v| v.starts_with("20"))
                .unwrap_or(false)
    }));

    // Should mount "out" directory for Next.js
    assert_eq!(plan.mounts[0].name, "out");
}

#[test]
fn test_detect_gatsby_fixture() {
    let path = fixture_path("node-gatsby");
    let provider = NodeStaticProvider;

    let result = provider.detect(&path).unwrap().unwrap();
    assert_eq!(result.confidence, 0.9);
    assert!(result.reason.contains("Gatsby"));
}

#[test]
fn test_plan_gatsby_fixture() {
    let path = fixture_path("node-gatsby");
    let provider = NodeStaticProvider;

    let plan = provider.plan(&path).unwrap();

    // Should use yarn (has yarn.lock)
    assert!(plan.build_steps[0].contains("yarn"));
    assert!(plan.dependencies.iter().any(|d| d.name == "yarn"));

    // Should mount "public" directory for Gatsby
    assert_eq!(plan.mounts[0].name, "public");
}

#[test]
fn test_detect_astro_fixture() {
    let path = fixture_path("node-astro");
    let provider = NodeStaticProvider;

    let result = provider.detect(&path).unwrap().unwrap();
    assert_eq!(result.confidence, 0.9);
    assert!(result.reason.contains("Astro"));
}

#[test]
fn test_plan_astro_fixture() {
    let path = fixture_path("node-astro");
    let provider = NodeStaticProvider;

    let plan = provider.plan(&path).unwrap();

    // Should use pnpm (has pnpm-lock.yaml)
    assert!(plan.build_steps[0].contains("pnpm"));
    assert!(plan.dependencies.iter().any(|d| d.name == "pnpm"));

    // Should mount "dist" directory for Astro
    assert_eq!(plan.mounts[0].name, "dist");
}

#[test]
fn test_detect_nuxt_fixture() {
    let path = fixture_path("node-nuxt");
    let provider = NodeStaticProvider;

    let result = provider.detect(&path).unwrap().unwrap();
    assert_eq!(result.confidence, 0.9);
    assert!(result.reason.contains("Nuxt"));
}

#[test]
fn test_plan_nuxt_fixture() {
    let path = fixture_path("node-nuxt");
    let provider = NodeStaticProvider;

    let plan = provider.plan(&path).unwrap();

    // Should use bun (has bun.lockb)
    assert!(plan.build_steps[0].contains("bun"));
    assert!(plan.dependencies.iter().any(|d| d.name == "bun"));

    // Should mount ".output/public" directory for Nuxt v3
    assert_eq!(plan.mounts[0].name, ".output/public");
}
