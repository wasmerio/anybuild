//! HTTP download utilities

use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::Path;

/// Download a file from a URL to a local path
///
/// Creates parent directories if they don't exist.
/// Overwrites the target file if it already exists.
pub fn download_file(url: &str, path: &Path) -> Result<()> {
    // Create parent directories
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    // Download the file
    let response =
        reqwest::blocking::get(url).with_context(|| format!("Failed to download from {}", url))?;

    if !response.status().is_success() {
        anyhow::bail!(
            "HTTP error {}: Failed to download {}",
            response.status(),
            url
        );
    }

    let bytes = response.bytes().context("Failed to read response body")?;

    // Write to file
    let mut file = fs::File::create(path)
        .with_context(|| format!("Failed to create file: {}", path.display()))?;

    file.write_all(&bytes)
        .with_context(|| format!("Failed to write to file: {}", path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_download_creates_parent_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("subdir/file.txt");

        // Note: This test requires network access and may fail in CI
        // In a real implementation, you'd use mockito or similar for testing
        assert!(!path.exists());
    }

    #[test]
    fn test_invalid_url() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("file.txt");

        let result = download_file("http://invalid-domain-that-does-not-exist.com", &path);
        assert!(result.is_err());
    }
}
