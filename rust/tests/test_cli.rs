//! CLI command tests using assert_cmd

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

mod common;

/// Create a Command for the shipit binary
/// Uses SHIPIT_BINARY env var if set, otherwise uses cargo-built binary
fn shipit_cmd() -> Command {
    if let Ok(custom_path) = std::env::var("SHIPIT_BINARY") {
        Command::new(custom_path)
    } else {
        Command::new(assert_cmd::cargo::cargo_bin!("shipit"))
    }
}

#[test]
fn test_version() {
    shipit_cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("shipit 0.17.2"));
}

#[test]
fn test_help() {
    shipit_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Shipit CLI"))
        .stdout(predicate::str::contains("Commands:"))
        .stdout(predicate::str::contains("auto"))
        .stdout(predicate::str::contains("generate"))
        .stdout(predicate::str::contains("build"))
        .stdout(predicate::str::contains("serve"))
        .stdout(predicate::str::contains("deploy"));
}

#[test]
fn test_generate_help() {
    shipit_cmd()
        .arg("generate")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Generate"))
        .stdout(predicate::str::contains("--provider"))
        .stdout(predicate::str::contains("--force"));
}

#[test]
fn test_build_help() {
    shipit_cmd()
        .arg("build")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Build"))
        .stdout(predicate::str::contains("--docker"));
}

#[test]
fn test_serve_help() {
    shipit_cmd()
        .arg("serve")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Serve"))
        .stdout(predicate::str::contains("--port"))
        .stdout(predicate::str::contains("--wasmer"))
        .stdout(predicate::str::contains("--start"));
}

#[test]
fn test_plan_help() {
    shipit_cmd()
        .arg("plan")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Show the build plan"))
        .stdout(predicate::str::contains("--format"));
}

#[test]
fn test_auto_help() {
    shipit_cmd()
        .arg("auto")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Detect, generate, build"))
        .stdout(predicate::str::contains("--skip-build"))
        .stdout(predicate::str::contains("--skip-serve"))
        .stdout(predicate::str::contains("--start"));
}

#[test]
fn test_deploy_help() {
    shipit_cmd()
        .arg("deploy")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Deploy"))
        .stdout(predicate::str::contains("--registry"))
        .stdout(predicate::str::contains("--app"));
}

#[test]
fn test_config_help() {
    shipit_cmd()
        .arg("config")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Manage configuration"));
}

#[test]
fn test_invalid_command() {
    // With default routing to auto, unknown subcommands are treated as paths
    // This will now route to auto and validate that the path exists
    shipit_cmd()
        .arg("invalid-command")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Path does not exist"));
}

#[test]
fn test_config_show() {
    shipit_cmd()
        .arg("config")
        .arg("show")
        .assert()
        .success()
        .stdout(predicate::str::contains("port"));
}

#[test]
fn test_config_path() {
    shipit_cmd()
        .arg("config")
        .arg("path")
        .assert()
        .success()
        .stdout(predicate::str::contains("Config paths"));
}

#[test]
fn test_generate_nonexistent_path() {
    shipit_cmd()
        .arg("generate")
        .arg("/nonexistent/path/that/does/not/exist")
        .assert()
        .failure();
}

#[test]
fn test_generate_dry_run_simple_project() {
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path();

    // Create a simple Python project
    fs::write(project_path.join("requirements.txt"), "fastapi\n").unwrap();
    fs::write(project_path.join("main.py"), "print('hello')").unwrap();

    shipit_cmd()
        .arg("generate")
        .arg("--dry-run")
        .arg(project_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Detecting"))
        .stdout(predicate::str::contains("python"));
}

#[test]
fn test_generate_creates_shipit_file() {
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path();

    // Create a simple static project
    fs::create_dir_all(project_path.join("public")).unwrap();
    fs::write(project_path.join("public/index.html"), "<h1>Hello</h1>").unwrap();

    shipit_cmd()
        .arg("generate")
        .arg(project_path)
        .assert()
        .success();

    // Check Shipit file was created
    let shipit_path = project_path.join("Shipit");
    assert!(shipit_path.exists(), "Shipit file should be created");

    let content = fs::read_to_string(shipit_path).unwrap();
    assert!(!content.is_empty(), "Shipit file should not be empty");
}

#[test]
fn test_generate_force_flag() {
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path();

    // Create a simple project
    fs::write(project_path.join("index.html"), "<h1>Test</h1>").unwrap();

    // Generate first time
    shipit_cmd()
        .arg("generate")
        .arg(project_path)
        .assert()
        .success();

    // Try again without force - should fail with error
    shipit_cmd()
        .arg("generate")
        .arg(project_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));

    // Force overwrite should succeed
    shipit_cmd()
        .arg("generate")
        .arg("--force")
        .arg(project_path)
        .assert()
        .success();
}

#[test]
fn test_plan_nonexistent_shipit() {
    let temp_dir = TempDir::new().unwrap();

    shipit_cmd()
        .arg("plan")
        .arg(temp_dir.path().join("Shipit"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("Path does not exist"));
}

#[test]
fn test_plan_directory_path_resolves_to_shipit() {
    let temp_dir = TempDir::new().unwrap();

    shipit_cmd()
        .arg("plan")
        .arg(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Shipit file not found"))
        .stderr(predicate::str::contains("Shipit"));
}

#[test]
fn test_plan_json_format() {
    let temp_dir = TempDir::new().unwrap();
    let shipit_path = temp_dir.path().join("Shipit");

    fs::write(
        &shipit_path,
        r#"python = dep("python", "3.11")
serve("app", "python", [], [python], {"start": "python app.py"})
"#,
    )
    .unwrap();

    shipit_cmd()
        .arg("plan")
        .arg("--format")
        .arg("json")
        .arg(&shipit_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"serve\""));
}

#[test]
fn test_build_nonexistent_shipit() {
    let temp_dir = TempDir::new().unwrap();

    shipit_cmd()
        .arg("build")
        .arg(temp_dir.path().join("Shipit"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("Path does not exist"));
}

#[test]
fn test_build_directory_path_resolves_to_shipit() {
    let temp_dir = TempDir::new().unwrap();

    shipit_cmd()
        .arg("build")
        .arg(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Shipit file not found"))
        .stderr(predicate::str::contains("Shipit"));
}

#[test]
fn test_deploy_directory_path_resolves_to_shipit() {
    let temp_dir = TempDir::new().unwrap();

    shipit_cmd()
        .arg("deploy")
        .arg(temp_dir.path())
        .arg("--app")
        .arg("test-app")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Shipit file not found"))
        .stderr(predicate::str::contains("Shipit"));
}

#[test]
fn test_serve_nonexistent_shipit() {
    let temp_dir = TempDir::new().unwrap();

    shipit_cmd()
        .arg("serve")
        .arg(temp_dir.path().join("Shipit"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("Path does not exist"));
}

#[test]
fn test_serve_directory_path_resolves_to_shipit() {
    let temp_dir = TempDir::new().unwrap();

    shipit_cmd()
        .arg("serve")
        .arg(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Shipit file not found"))
        .stderr(predicate::str::contains("Shipit"));
}

#[test]
fn test_serve_start_flag_is_accepted() {
    let temp_dir = TempDir::new().unwrap();

    shipit_cmd()
        .arg("serve")
        .arg(temp_dir.path().join("Shipit"))
        .arg("--start")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Path does not exist"));
}

#[test]
fn test_auto_start_flag_is_accepted() {
    shipit_cmd()
        .arg("auto")
        .arg("/nonexistent/path/that/does/not/exist")
        .arg("--wasmer")
        .arg("--start")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unexpected argument").not());
}
