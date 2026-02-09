//! Integration tests for Laravel provider using real project fixtures.

use shipit::providers::ProviderRegistry;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn test_laravel_detection() {
    let path = fixtures_dir().join("laravel-app");
    let registry = ProviderRegistry::with_defaults();

    let result = registry.detect_best(&path).unwrap();
    assert!(result.is_some());

    let (provider, detect_result) = result.unwrap();
    assert_eq!(provider.name(), "laravel");
    assert_eq!(detect_result.confidence, 1.0);
}

#[test]
fn test_laravel_plan() {
    let path = fixtures_dir().join("laravel-app");
    let registry = ProviderRegistry::with_defaults();

    let (provider, _) = registry.detect_best(&path).unwrap().unwrap();
    let plan = provider.plan(&path).unwrap();

    assert_eq!(plan.serve_name, "public");
    assert!(plan.dependencies.iter().any(|d| d.name == "php"));
    assert!(plan.dependencies.iter().any(|d| d.name == "node"));
    assert!(plan.build_steps[0].contains("composer install"));
    assert!(plan.build_steps.iter().any(|s| s.contains("npm install")));
    assert!(plan.commands.get("web").unwrap().contains("artisan serve"));
}

#[test]
fn test_laravel_vite_build() {
    let path = fixtures_dir().join("laravel-app");
    let registry = ProviderRegistry::with_defaults();

    let (provider, _) = registry.detect_best(&path).unwrap().unwrap();
    let plan = provider.plan(&path).unwrap();

    // Should use npm run build for Vite
    assert!(plan.build_steps.iter().any(|s| s.contains("npm run build")));
}

#[test]
fn test_laravel_artisan_cache() {
    let path = fixtures_dir().join("laravel-app");
    let registry = ProviderRegistry::with_defaults();

    let (provider, _) = registry.detect_best(&path).unwrap().unwrap();
    let plan = provider.plan(&path).unwrap();

    // Should have artisan cache commands
    assert!(plan.build_steps.iter().any(|s| s.contains("config:cache")));
    assert!(plan.build_steps.iter().any(|s| s.contains("route:cache")));
    assert!(plan.build_steps.iter().any(|s| s.contains("view:cache")));
}

#[test]
fn test_laravel_php_version() {
    let path = fixtures_dir().join("laravel-app");
    let registry = ProviderRegistry::with_defaults();

    let (provider, _) = registry.detect_best(&path).unwrap().unwrap();
    let plan = provider.plan(&path).unwrap();

    let php_dep = plan.dependencies.iter().find(|d| d.name == "php").unwrap();
    assert_eq!(php_dep.default_version, Some("8.2".to_string()));
}
