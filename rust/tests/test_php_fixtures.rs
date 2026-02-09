//! Integration tests for PHP provider using real project fixtures.

use shipit::providers::ProviderRegistry;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn test_php_api_detection() {
    let path = fixtures_dir().join("php-api");
    let registry = ProviderRegistry::with_defaults();

    let result = registry.detect_best(&path).unwrap();
    assert!(result.is_some());

    let (provider, detect_result) = result.unwrap();
    assert_eq!(provider.name(), "php");
    assert_eq!(detect_result.confidence, 0.8);
}

#[test]
fn test_php_api_plan() {
    let path = fixtures_dir().join("php-api");
    let registry = ProviderRegistry::with_defaults();

    let (provider, _) = registry.detect_best(&path).unwrap().unwrap();
    let plan = provider.plan(&path).unwrap();

    assert_eq!(plan.serve_name, "public");
    assert!(plan.dependencies.iter().any(|d| d.name == "php"));
    assert!(plan.build_steps[0].contains("composer install"));
    assert!(plan.commands.get("web").unwrap().contains("php -S"));
}

#[test]
fn test_php_version_detection() {
    let path = fixtures_dir().join("php-api");
    let registry = ProviderRegistry::with_defaults();

    let (provider, _) = registry.detect_best(&path).unwrap().unwrap();
    let plan = provider.plan(&path).unwrap();

    let php_dep = plan.dependencies.iter().find(|d| d.name == "php").unwrap();
    assert_eq!(php_dep.default_version, Some("8.1".to_string()));
}
