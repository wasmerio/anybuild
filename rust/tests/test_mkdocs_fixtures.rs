//! Integration tests for MkDocs provider using real project fixtures.

use shipit::providers::ProviderRegistry;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn test_mkdocs_detection() {
    let path = fixtures_dir().join("mkdocs-site");
    let registry = ProviderRegistry::with_defaults();

    let result = registry.detect_best(&path).unwrap();
    assert!(result.is_some());

    let (provider, detect_result) = result.unwrap();
    assert_eq!(provider.name(), "mkdocs");
    assert_eq!(detect_result.confidence, 1.0);
}

#[test]
fn test_mkdocs_plan() {
    let path = fixtures_dir().join("mkdocs-site");
    let registry = ProviderRegistry::with_defaults();

    let (provider, _) = registry.detect_best(&path).unwrap().unwrap();
    let plan = provider.plan(&path).unwrap();

    assert_eq!(plan.serve_name, "site");
    assert!(plan.dependencies.iter().any(|d| d.name == "python"));
    assert!(plan.build_steps[0].contains("pip install"));
    assert!(plan.build_steps[1].contains("mkdocs build"));
}

#[test]
fn test_mkdocs_with_requirements() {
    let path = fixtures_dir().join("mkdocs-site");
    let registry = ProviderRegistry::with_defaults();

    let (provider, _) = registry.detect_best(&path).unwrap().unwrap();
    let plan = provider.plan(&path).unwrap();

    // Should use requirements.txt since it exists
    assert!(plan.build_steps[0].contains("requirements.txt"));
}
