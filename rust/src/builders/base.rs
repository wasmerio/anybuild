//! Build backend trait and common utilities.

use crate::types::{Mount, Step};
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Build backend trait for executing build steps.
///
/// Implementations can build locally or in containers.
pub trait BuildBackend {
    /// Get the mount path for a named mount (during build).
    fn get_build_mount_path(&self, name: &str) -> PathBuf;

    /// Get the artifact mount path (after build, for export).
    fn get_artifact_mount_path(&self, name: &str) -> PathBuf;

    /// Execute a single build step.
    fn execute_step(&mut self, step: &Step, env: &mut HashMap<String, String>) -> Result<()>;

    /// Build all steps.
    fn build(
        &mut self,
        name: &str,
        env: HashMap<String, String>,
        mounts: &[Mount],
        steps: &[Step],
    ) -> Result<()>;

    /// Get the runtime PATH after build (if modified).
    fn get_runtime_path(&self) -> Option<String>;
}

/// Normalize and sanitize a path.
pub fn sanitize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        use std::path::Component;
        match component {
            Component::Normal(c) => components.push(c),
            Component::RootDir => components.clear(),
            Component::ParentDir => {
                components.pop();
            }
            Component::CurDir => {}
            Component::Prefix(_) => {}
        }
    }
    components.iter().collect()
}

/// Ensure a directory exists, creating it if necessary.
pub fn ensure_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

/// Copy files with ignore patterns.
pub fn copy_with_ignore(src: &Path, dst: &Path, patterns: &[String]) -> Result<()> {
    use ignore::WalkBuilder;

    // Build the walker with ignore patterns
    let mut builder = WalkBuilder::new(src);

    // Add patterns to ignore
    for pattern in patterns {
        builder.add_custom_ignore_filename(pattern);
    }

    // Always ignore .shipit directory and Shipit file
    builder.filter_entry(|entry| {
        let file_name = entry.file_name().to_string_lossy();
        !matches!(file_name.as_ref(), ".shipit" | "Shipit")
    });

    ensure_dir(dst)?;

    for entry in builder.build() {
        let entry = entry?;
        let path = entry.path();

        if path == src {
            continue; // Skip the root directory itself
        }

        let relative = path.strip_prefix(src)?;
        let target = dst.join(relative);

        if entry.file_type().is_some_and(|ft| ft.is_dir()) {
            ensure_dir(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                ensure_dir(parent)?;
            }
            std::fs::copy(path, &target)?;
        }
    }

    Ok(())
}

/// Merge two environment variable maps.
pub fn merge_env(
    base: HashMap<String, String>,
    extra: HashMap<String, String>,
) -> HashMap<String, String> {
    let mut result = base;
    result.extend(extra);
    result
}

/// Extend the PATH environment variable.
pub fn extend_path(current_path: Option<&str>, new_path: &str) -> String {
    let separator = if cfg!(windows) { ";" } else { ":" };

    if let Some(current) = current_path {
        format!("{}{}{}", new_path, separator, current)
    } else {
        new_path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_path() {
        let path = Path::new("/foo/../bar/./baz");
        let sanitized = sanitize_path(path);
        assert_eq!(sanitized, PathBuf::from("bar/baz"));
    }

    #[test]
    fn test_extend_path_empty() {
        let result = extend_path(None, "/usr/local/bin");
        assert_eq!(result, "/usr/local/bin");
    }

    #[test]
    fn test_extend_path_existing() {
        let result = extend_path(Some("/usr/bin:/bin"), "/usr/local/bin");

        #[cfg(unix)]
        assert_eq!(result, "/usr/local/bin:/usr/bin:/bin");

        #[cfg(windows)]
        assert_eq!(result, "/usr/local/bin;/usr/bin:/bin");
    }

    #[test]
    fn test_merge_env() {
        let mut base = HashMap::new();
        base.insert("FOO".to_string(), "bar".to_string());
        base.insert("BAZ".to_string(), "old".to_string());

        let mut extra = HashMap::new();
        extra.insert("BAZ".to_string(), "new".to_string());
        extra.insert("QUX".to_string(), "value".to_string());

        let result = merge_env(base, extra);

        assert_eq!(result.get("FOO"), Some(&"bar".to_string()));
        assert_eq!(result.get("BAZ"), Some(&"new".to_string()));
        assert_eq!(result.get("QUX"), Some(&"value".to_string()));
    }
}
