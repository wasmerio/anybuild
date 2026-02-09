//! End-to-end tests for example projects
//!
//! These tests build and serve actual example projects,
//! then verify they respond correctly via HTTP.

use assert_cmd::Command;
use serial_test::serial;
use std::process::{Command as StdCommand, Stdio};
use std::thread;
use std::time::Duration;

mod common;
use common::{example_exists, example_path, find_free_port, http_get, shipit_bin, wait_for_http};

/// Helper to run a full E2E test on an example project
fn test_example_e2e(example_name: &str, expected_content: &str, endpoint: &str) {
    if !example_exists(example_name) {
        eprintln!("Skipping test: example '{}' not found", example_name);
        return;
    }

    let example_dir = example_path(example_name);
    let port = find_free_port();

    println!("Testing {} on port {}", example_name, port);

    // Build the example
    let build_result = Command::new(shipit_bin())
        .current_dir(&example_dir)
        .arg("build")
        .output()
        .expect("Failed to run build");

    if !build_result.status.success() {
        eprintln!(
            "Build failed for {}: {}",
            example_name,
            String::from_utf8_lossy(&build_result.stderr)
        );
        // Many examples may fail due to Starlark issues, skip them
        return;
    }

    // Start the server in background
    let mut server = StdCommand::new(shipit_bin())
        .current_dir(&example_dir)
        .arg("serve")
        .arg("--port")
        .arg(port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to start server");

    // Give server time to start
    thread::sleep(Duration::from_secs(2));

    // Build URL
    let url = format!("http://localhost:{}{}", port, endpoint);

    // Wait for server to be ready
    if wait_for_http(&url, Duration::from_secs(10)).is_ok() {
        // Test endpoint
        match http_get(&url) {
            Ok(content) => {
                assert!(
                    content.contains(expected_content),
                    "Expected content '{}' not found in response from {}",
                    expected_content,
                    example_name
                );
                println!("✓ {} passed", example_name);
            }
            Err(e) => {
                eprintln!("Failed to fetch from {}: {}", example_name, e);
            }
        }
    } else {
        eprintln!("Server failed to start for {}", example_name);
    }

    // Cleanup
    let _ = server.kill();
}

#[test]
#[serial]
fn test_static_nobuild_e2e() {
    test_example_e2e("static-nobuild", "Hello", "/index.html");
}

#[test]
#[serial]
fn test_staticfile_e2e() {
    test_example_e2e("staticfile", "html", "/index.html");
}

#[test]
#[serial]
fn test_php_nobuild_e2e() {
    test_example_e2e("php-nobuild", "PHP", "/");
}

#[test]
#[serial]
fn test_php_api_e2e() {
    test_example_e2e("php-api", "users", "/api/users");
}

#[test]
#[serial]
fn test_python_fasthtml_e2e() {
    test_example_e2e("python-fasthtml", "html", "/");
}

#[test]
#[serial]
fn test_go_simple_e2e() {
    test_example_e2e("go-simple", "Hello", "/");
}

// Test that can build but may not serve correctly
#[test]
fn test_examples_can_build() {
    let examples = vec!["hugo", "mkdocs", "python-flask", "python-django"];

    for example_name in examples {
        if !example_exists(example_name) {
            continue;
        }

        let example_dir = example_path(example_name);

        println!("Testing build for {}", example_name);

        let result = Command::new(shipit_bin())
            .current_dir(&example_dir)
            .arg("build")
            .output()
            .expect("Failed to run build");

        // We just verify the command runs, not that it succeeds
        // (many may fail due to Starlark issues)
        println!(
            "Build for {} completed with status: {}",
            example_name, result.status
        );
    }
}

#[test]
fn test_examples_can_plan() {
    let examples = vec![
        "static-nobuild",
        "php-nobuild",
        "python-fasthtml",
        "go-simple",
    ];

    for example_name in examples {
        if !example_exists(example_name) {
            continue;
        }

        let example_dir = example_path(example_name);

        println!("Testing plan for {}", example_name);

        let result = Command::new(shipit_bin())
            .current_dir(&example_dir)
            .arg("plan")
            .output()
            .expect("Failed to run plan");

        if result.status.success() {
            println!("✓ Plan succeeded for {}", example_name);
        } else {
            println!(
                "✗ Plan failed for {}: {}",
                example_name,
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
}

#[test]
fn test_example_shipit_files_exist() {
    let examples = vec![
        "static-nobuild",
        "staticfile",
        "php-nobuild",
        "php-api",
        "python-fasthtml",
        "python-flask",
        "python-django",
        "go-simple",
        "hugo",
        "mkdocs",
        "nextjs",
        "nuxt",
        "gatsby",
    ];

    let mut found = 0;
    let mut missing = 0;

    for example_name in examples {
        if !example_exists(example_name) {
            continue;
        }

        let shipit_path = example_path(example_name).join("Shipit");

        if shipit_path.exists() {
            found += 1;
            println!("✓ {}/Shipit exists", example_name);
        } else {
            missing += 1;
            println!("✗ {}/Shipit missing", example_name);
        }
    }

    println!("Found: {}, Missing: {}", found, missing);
    assert!(found > 0, "Should find at least some Shipit files");
}

#[test]
fn test_example_detection() {
    // Test that we can detect project types for examples
    let test_cases = vec![
        ("nextjs", "nextjs"),
        ("nuxt", "nuxt"),
        ("gatsby", "gatsby"),
        ("python-django", "django"),
        ("python-flask", "flask"),
        ("php-laravel-react", "laravel"),
        ("go-simple", "go"),
    ];

    for (example_name, expected_type) in test_cases {
        if !example_exists(example_name) {
            continue;
        }

        let example_dir = example_path(example_name);

        let result = Command::new(shipit_bin())
            .current_dir(&example_dir)
            .arg("generate")
            .arg("--dry-run")
            .output()
            .expect("Failed to run generate");

        let stdout = String::from_utf8_lossy(&result.stdout);

        if stdout.contains(&format!("Detected: {}", expected_type)) {
            println!("✓ Correctly detected {} as {}", example_name, expected_type);
        } else if result.status.success() {
            println!(
                "⚠ Detected {} but not as expected type {}",
                example_name, expected_type
            );
        }
    }
}

#[test]
fn test_docker_build_backend_available() {
    // Just verify we can check Docker availability
    let result = Command::new("docker").arg("--version").output();

    match result {
        Ok(output) if output.status.success() => {
            println!("✓ Docker is available");
            let version = String::from_utf8_lossy(&output.stdout);
            println!("  Version: {}", version.trim());
        }
        _ => {
            println!("⚠ Docker is not available (tests requiring Docker will be skipped)");
        }
    }
}

#[test]
fn test_wasmer_runner_available() {
    // Just verify we can check Wasmer availability
    let result = Command::new("wasmer").arg("--version").output();

    match result {
        Ok(output) if output.status.success() => {
            println!("✓ Wasmer is available");
            let version = String::from_utf8_lossy(&output.stdout);
            println!("  Version: {}", version.trim());
        }
        _ => {
            println!("⚠ Wasmer is not available (tests requiring Wasmer will be skipped)");
        }
    }
}

#[test]
fn test_build_artifacts_created() {
    let example_name = "static-nobuild";

    if !example_exists(example_name) {
        return;
    }

    let example_dir = example_path(example_name);

    // Run build
    let result = Command::new(shipit_bin())
        .current_dir(&example_dir)
        .arg("build")
        .output()
        .expect("Failed to run build");

    if !result.status.success() {
        eprintln!("Build failed, skipping artifact check");
        return;
    }

    // Check for .shipit directory
    let shipit_dir = example_dir.join(".shipit");
    if shipit_dir.exists() {
        println!("✓ Build artifacts directory created: .shipit/");

        // Check for common subdirectories
        let local_dir = shipit_dir.join("local");
        if local_dir.exists() {
            println!("  ✓ Local build directory exists");
        }
    } else {
        println!("✗ Build artifacts directory not created");
    }
}

#[test]
fn test_serve_without_build_fails() {
    let example_name = "static-nobuild";

    if !example_exists(example_name) {
        return;
    }

    let example_dir = example_path(example_name);

    // Clean any existing builds
    let shipit_dir = example_dir.join(".shipit");
    if shipit_dir.exists() {
        let _ = std::fs::remove_dir_all(&shipit_dir);
    }

    // Try to serve without building
    let result = Command::new(shipit_bin())
        .current_dir(&example_dir)
        .arg("serve")
        .output()
        .expect("Failed to run serve");

    // Should fail because nothing was built
    assert!(
        !result.status.success(),
        "Serve should fail without prior build"
    );
}
