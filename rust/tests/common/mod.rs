//! Common test utilities and helpers
//!
//! These functions are shared across multiple test binaries.
//! Some may be unused in individual test files but are used elsewhere.
#![allow(dead_code)]

use anyhow::{Context, Result};
use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use tempfile::TempDir;

/// Create a temporary project directory with files
pub fn create_temp_project(files: Vec<(&str, &str)>) -> Result<TempDir> {
    let temp_dir = TempDir::new().context("Failed to create temp directory")?;

    for (path, content) in files {
        let file_path = temp_dir.path().join(path);

        // Create parent directories if needed
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create parent dir: {}", parent.display()))?;
        }

        fs::write(&file_path, content)
            .with_context(|| format!("Failed to write file: {}", file_path.display()))?;
    }

    Ok(temp_dir)
}

/// Find a free port on localhost
pub fn find_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind to any port")
        .local_addr()
        .expect("Failed to get local address")
        .port()
}

/// Make an HTTP GET request (blocking)
pub fn http_get(url: &str) -> Result<String> {
    let response = reqwest::blocking::get(url).with_context(|| format!("Failed to GET {}", url))?;

    let status = response.status();
    let body = response.text().context("Failed to read response body")?;

    if !status.is_success() {
        anyhow::bail!("HTTP request failed with status {}: {}", status, body);
    }

    Ok(body)
}

/// Wait for HTTP server to be ready
pub fn wait_for_http(url: &str, timeout: std::time::Duration) -> Result<()> {
    let start = std::time::Instant::now();
    let mut last_error = None;

    while start.elapsed() < timeout {
        match reqwest::blocking::get(url) {
            Ok(_) => return Ok(()),
            Err(e) => {
                last_error = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }
    }

    Err(anyhow::anyhow!(
        "Server did not become ready within {:?}: {:?}",
        timeout,
        last_error
    ))
}

/// Get the path to the shipit binary
///
/// Can be overridden with SHIPIT_BINARY env var to test alternative
/// implementations (e.g., Python version). Defaults to cargo-built binary.
pub fn shipit_bin() -> PathBuf {
    if let Ok(custom_path) = std::env::var("SHIPIT_BINARY") {
        return PathBuf::from(custom_path);
    }

    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .and_then(|p| {
            // In tests, binary is in target/debug/deps, go up to debug
            if p.ends_with("deps") {
                p.parent().map(|p| p.join("shipit"))
            } else {
                Some(p.join("shipit"))
            }
        })
        .unwrap_or_else(|| PathBuf::from("target/debug/shipit"))
}

/// Get the workspace root (where Cargo.toml is)
pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to get workspace root")
        .to_path_buf()
}

/// Get path to examples directory
pub fn examples_dir() -> PathBuf {
    workspace_root().join("examples")
}

/// Get path to a specific example
pub fn example_path(name: &str) -> PathBuf {
    examples_dir().join(name)
}

/// Check if an example exists
pub fn example_exists(name: &str) -> bool {
    example_path(name).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_temp_project() {
        let temp =
            create_temp_project(vec![("test.txt", "hello"), ("dir/nested.txt", "world")]).unwrap();

        assert!(temp.path().join("test.txt").exists());
        assert!(temp.path().join("dir/nested.txt").exists());

        let content = fs::read_to_string(temp.path().join("test.txt")).unwrap();
        assert_eq!(content, "hello");
    }

    #[test]
    fn test_find_free_port() {
        let port1 = find_free_port();
        let port2 = find_free_port();

        assert!(port1 > 0);
        assert!(port2 > 0);
        // Ports should be different (usually)
        assert_ne!(port1, port2);
    }

    #[test]
    fn test_workspace_paths() {
        let root = workspace_root();
        assert!(root.exists());

        let examples = examples_dir();
        assert!(examples.exists());
        assert!(examples.is_dir());
    }
}
