//! Provider trait and base utilities

use crate::providers::{DetectResult, ProviderPlan};
use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

/// Provider trait for detecting and planning builds
pub trait Provider: Send + Sync {
    /// Provider name (e.g., "python", "node-static")
    fn name(&self) -> &str;

    /// Detect if this provider applies to the project
    ///
    /// Returns Some(DetectResult) if the provider matches, None otherwise
    fn detect(&self, project_path: &Path) -> Result<Option<DetectResult>>;

    /// Generate build/serve plan for the project
    fn plan(&self, project_path: &Path) -> Result<ProviderPlan>;

    /// Provider priority (higher = checked first)
    ///
    /// Default priority is 0. Use negative values for fallback providers.
    fn priority(&self) -> i32 {
        0
    }
}

/// Check if any of the candidate paths exist
pub fn exists(path: &Path, candidates: &[&str]) -> bool {
    candidates.iter().any(|c| path.join(c).exists())
}

/// Check if a file contains a pattern
pub fn file_contains(path: &Path, pattern: &str) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    Ok(contents.contains(pattern))
}

/// Find files matching a glob pattern
pub fn find_files(dir: &Path, pattern: &str) -> Result<Vec<PathBuf>> {
    use walkdir::WalkDir;

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let glob_pattern = glob::Pattern::new(pattern)
        .with_context(|| format!("Invalid glob pattern: {}", pattern))?;

    let mut results = Vec::new();

    for entry in WalkDir::new(dir).max_depth(5) {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        if let Some(file_name) = path.file_name() {
            if glob_pattern.matches(file_name.to_string_lossy().as_ref()) {
                results.push(path.to_path_buf());
            }
        }
    }

    Ok(results)
}

/// Read and parse a JSON file
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read JSON file: {}", path.display()))?;

    serde_json::from_str(&contents)
        .with_context(|| format!("Failed to parse JSON: {}", path.display()))
}

/// Read and parse a TOML file
pub fn read_toml<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read TOML file: {}", path.display()))?;

    toml::from_str(&contents).with_context(|| format!("Failed to parse TOML: {}", path.display()))
}

/// Read and parse a YAML file
pub fn read_yaml<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read YAML file: {}", path.display()))?;

    serde_yaml::from_str(&contents)
        .with_context(|| format!("Failed to parse YAML: {}", path.display()))
}

/// Check if a file is executable
#[cfg(unix)]
pub fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = std::fs::metadata(path) {
        let permissions = metadata.permissions();
        permissions.mode() & 0o111 != 0
    } else {
        false
    }
}

/// Check if a file is executable (Windows always returns false)
#[cfg(not(unix))]
pub fn is_executable(_path: &Path) -> bool {
    false
}

/// Get file size in bytes
pub fn get_file_size(path: &Path) -> Result<u64> {
    let metadata = std::fs::metadata(path)
        .with_context(|| format!("Failed to get metadata for: {}", path.display()))?;
    Ok(metadata.len())
}

/// Detect by checking if a specific file exists
pub fn detect_by_file(path: &Path, filename: &str) -> bool {
    path.join(filename).exists()
}

/// Detect by checking if any files match a glob pattern
pub fn detect_by_pattern(path: &Path, glob: &str) -> bool {
    find_files(path, glob)
        .map(|files| !files.is_empty())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_exists() {
        let temp = TempDir::new().unwrap();
        let path = temp.path();

        fs::write(path.join("file1.txt"), "content").unwrap();
        fs::write(path.join("file2.txt"), "content").unwrap();

        assert!(exists(path, &["file1.txt", "file3.txt"]));
        assert!(!exists(path, &["file3.txt", "file4.txt"]));
    }

    #[test]
    fn test_file_contains() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("test.txt");

        fs::write(&file, "hello world\nfoo bar").unwrap();

        assert!(file_contains(&file, "hello").unwrap());
        assert!(file_contains(&file, "foo bar").unwrap());
        assert!(!file_contains(&file, "missing").unwrap());
    }

    #[test]
    fn test_find_files() {
        let temp = TempDir::new().unwrap();
        let path = temp.path();

        fs::write(path.join("test.txt"), "").unwrap();
        fs::write(path.join("test.md"), "").unwrap();
        fs::write(path.join("other.rs"), "").unwrap();

        let files = find_files(path, "*.txt").unwrap();
        assert_eq!(files.len(), 1);

        let files = find_files(path, "test.*").unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_read_json() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("test.json");

        fs::write(&file, r#"{"name": "test", "value": 42}"#).unwrap();

        #[derive(serde::Deserialize)]
        struct TestData {
            name: String,
            value: i32,
        }

        let data: TestData = read_json(&file).unwrap();
        assert_eq!(data.name, "test");
        assert_eq!(data.value, 42);
    }

    #[test]
    fn test_detect_by_file() {
        let temp = TempDir::new().unwrap();
        let path = temp.path();

        fs::write(path.join("package.json"), "{}").unwrap();

        assert!(detect_by_file(path, "package.json"));
        assert!(!detect_by_file(path, "Cargo.toml"));
    }

    #[test]
    fn test_detect_by_pattern() {
        let temp = TempDir::new().unwrap();
        let path = temp.path();

        fs::write(path.join("test.py"), "").unwrap();
        fs::write(path.join("main.py"), "").unwrap();

        assert!(detect_by_pattern(path, "*.py"));
        assert!(!detect_by_pattern(path, "*.rs"));
    }
}
