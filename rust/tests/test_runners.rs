//! Integration tests for runners

use shipit::{
    builders::LocalBuildBackend,
    runners::{LocalRunner, Runner},
    types::{serve::Serve, RunStep},
};
use std::collections::HashMap;
use std::sync::Arc;

#[test]
fn test_local_runner_with_simple_serve() {
    // Create a minimal LocalBuildBackend
    let temp_dir = std::env::temp_dir().join("shipit-test-runner");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let backend = LocalBuildBackend::new(temp_dir.clone(), temp_dir.clone());

    // Create a simple Serve configuration
    let mut commands = HashMap::new();
    commands.insert("web".to_string(), "echo 'hello'".to_string());

    let serve = Serve {
        name: "test".to_string(),
        provider: "static".to_string(),
        build: vec![],
        deps: vec![],
        commands,
        cwd: None,
        env: None,
        mounts: None,
        prepare: Some(vec![RunStep::new("echo 'preparing'")]),
        workers: None,
        volumes: None,
        services: None,
    };

    // Create LocalRunner
    let mut runner = LocalRunner::new(Arc::new(backend), temp_dir.clone());

    // Build (generates prepare and serve scripts)
    runner.build(&serve).unwrap();

    // Check that prepare script was created
    let prepare_script = temp_dir.join(".shipit/runner/local/prepare/prepare.sh");
    assert!(prepare_script.exists());

    // Check that serve script for 'web' command was created
    let serve_script = temp_dir.join(".shipit/runner/local/serve/bin/web");
    assert!(serve_script.exists());

    // Cleanup
    std::fs::remove_dir_all(&temp_dir).ok();
}

#[test]
fn test_local_runner_prepare_script_generation() {
    let temp_dir = std::env::temp_dir().join("shipit-test-prepare");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let backend = LocalBuildBackend::new(temp_dir.clone(), temp_dir.clone());

    let mut commands = HashMap::new();
    commands.insert("web".to_string(), "python -m http.server".to_string());

    let serve = Serve {
        name: "test".to_string(),
        provider: "python".to_string(),
        build: vec![],
        deps: vec![],
        commands,
        cwd: Some("/app".to_string()),
        env: None,
        mounts: None,
        prepare: None,
        workers: None,
        volumes: None,
        services: None,
    };

    let mut runner = LocalRunner::new(Arc::new(backend), temp_dir.clone());
    runner.build(&serve).unwrap();

    // Read the generated web serve script
    let serve_script = temp_dir.join(".shipit/runner/local/serve/bin/web");
    let content = std::fs::read_to_string(&serve_script).unwrap();

    // Verify it contains the command
    assert!(content.contains("python -m http.server"));

    // Verify it has proper bash setup (using #!/bin/bash, not #!/usr/bin/env bash)
    assert!(content.contains("#!/bin/bash"));
    assert!(content.contains("set -e"));

    // Verify it changes to the working directory
    assert!(content.contains("cd /app"));

    // Cleanup
    std::fs::remove_dir_all(&temp_dir).ok();
}

#[test]
fn test_runner_mount_paths() {
    let temp_dir = std::env::temp_dir().join("shipit-test-mounts");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let backend = LocalBuildBackend::new(temp_dir.clone(), temp_dir.clone());

    let runner = LocalRunner::new(Arc::new(backend), temp_dir.clone());

    // Get serve mount path for the "output" mount
    let mount_path = runner.get_serve_mount_path("output");

    // Mount path should be the local artifact directory
    assert!(mount_path.to_string_lossy().contains(".shipit"));

    // Cleanup
    std::fs::remove_dir_all(&temp_dir).ok();
}
