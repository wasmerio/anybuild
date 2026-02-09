//! Integration tests for Hugo provider using real project fixtures.

use shipit::providers::ProviderRegistry;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn test_hugo_detection() {
    let path = fixtures_dir().join("hugo-site");
    let registry = ProviderRegistry::with_defaults();

    let result = registry.detect_best(&path).unwrap();
    assert!(result.is_some());

    let (provider, detect_result) = result.unwrap();
    assert_eq!(provider.name(), "hugo");
    assert_eq!(detect_result.confidence, 1.0);
}

#[test]
fn test_hugo_plan() {
    let path = fixtures_dir().join("hugo-site");
    let registry = ProviderRegistry::with_defaults();

    let (provider, _) = registry.detect_best(&path).unwrap().unwrap();
    let plan = provider.plan(&path).unwrap();

    assert_eq!(plan.serve_name, "public");
    assert!(plan.dependencies.iter().any(|d| d.name == "hugo"));
    assert!(plan.build_steps[0].contains("hugo"));
    assert!(plan.mounts.iter().any(|m| m.name == "public"));
}
