//! Integration tests for Python provider with real project fixtures.

use shipit::providers::{base::Provider, python::PythonProvider};
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn test_detect_django_fixture() {
    let path = fixture_path("python-django");
    let provider = PythonProvider;

    let result = provider.detect(&path).unwrap().unwrap();
    assert_eq!(result.name, "python");
    assert_eq!(result.confidence, 1.0);
    assert!(result.reason.contains("Django"));
}

#[test]
fn test_plan_django_fixture() {
    let path = fixture_path("python-django");
    let provider = PythonProvider;

    let plan = provider.plan(&path).unwrap();

    // Should install from requirements.txt
    assert!(plan.build_steps[0].contains("pip install"));

    // Should have collectstatic for Django
    assert!(plan.build_steps.iter().any(|s| s.contains("collectstatic")));

    // Should have gunicorn web command
    assert!(plan.commands.get("web").unwrap().contains("gunicorn"));
}

#[test]
fn test_detect_fastapi_fixture() {
    let path = fixture_path("python-fastapi");
    let provider = PythonProvider;

    let result = provider.detect(&path).unwrap().unwrap();
    assert_eq!(result.confidence, 1.0);
    assert!(result.reason.contains("FastAPI"));
}

#[test]
fn test_plan_fastapi_fixture() {
    let path = fixture_path("python-fastapi");
    let provider = PythonProvider;

    let plan = provider.plan(&path).unwrap();

    // Should have uvicorn web command
    assert!(plan.commands.get("web").unwrap().contains("uvicorn"));
}

#[test]
fn test_detect_flask_fixture() {
    let path = fixture_path("python-flask");
    let provider = PythonProvider;

    let result = provider.detect(&path).unwrap().unwrap();
    assert_eq!(result.confidence, 1.0);
    assert!(result.reason.contains("Flask"));
}

#[test]
fn test_plan_flask_fixture() {
    let path = fixture_path("python-flask");
    let provider = PythonProvider;

    let plan = provider.plan(&path).unwrap();

    // Should have gunicorn web command
    assert!(plan.commands.get("web").unwrap().contains("gunicorn"));
}
