//! Integration tests for Go provider using real project fixtures.

use shipit::providers::ProviderRegistry;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn test_go_simple_detection() {
    let path = fixtures_dir().join("go-simple");
    let registry = ProviderRegistry::with_defaults();

    let result = registry.detect_best(&path).unwrap();
    assert!(result.is_some());

    let (provider, detect_result) = result.unwrap();
    assert_eq!(provider.name(), "go");
    assert_eq!(detect_result.confidence, 1.0);
}

#[test]
fn test_go_simple_plan() {
    let path = fixtures_dir().join("go-simple");
    let registry = ProviderRegistry::with_defaults();

    let (provider, _) = registry.detect_best(&path).unwrap().unwrap();
    let plan = provider.plan(&path).unwrap();

    assert_eq!(plan.serve_name, "app");
    assert!(plan.dependencies.iter().any(|d| d.name == "go"));
    assert!(plan.build_steps[0].contains("go mod download"));
    assert!(plan.build_steps[1].contains("go build"));
    assert!(plan.commands.get("web").is_some());
}

#[test]
fn test_go_version_detection() {
    let path = fixtures_dir().join("go-simple");
    let registry = ProviderRegistry::with_defaults();

    let (provider, _) = registry.detect_best(&path).unwrap().unwrap();
    let plan = provider.plan(&path).unwrap();

    let go_dep = plan.dependencies.iter().find(|d| d.name == "go").unwrap();
    assert_eq!(go_dep.default_version, Some("1.21".to_string()));
}
