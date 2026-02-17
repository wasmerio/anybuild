//! Path utilities for cross-platform compatibility

use std::path::{Path, PathBuf};

/// Resolve a Shipit path input.
///
/// If `path` is a directory, returns `<path>/Shipit`.
/// Otherwise returns the path unchanged.
pub fn resolve_shipit_path(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.join("Shipit")
    } else {
        path.to_path_buf()
    }
}

/// Resolve the Shipit file path given a project path and optional override.
///
/// This matches Python CLI behavior and supports flexible input:
/// - If `shipit_path_override` is provided, use it (can be absolute or relative to project_path)
/// - Otherwise, if `project_path` is a file, use it directly
/// - Otherwise, treat `project_path` as a directory and return `<project_path>/Shipit`
///
/// # Arguments
///
/// * `project_path` - The project directory path or direct Shipit file path
/// * `shipit_path_override` - Optional explicit Shipit file path
pub fn resolve_shipit_path_with_override(
    project_path: &Path,
    shipit_path_override: Option<&Path>,
) -> PathBuf {
    match shipit_path_override {
        Some(override_path) => {
            // If absolute, use as-is; if relative, resolve relative to project path
            if override_path.is_absolute() {
                override_path.to_path_buf()
            } else {
                project_path.join(override_path)
            }
        }
        None => {
            // If the path is a file, use it directly
            // Otherwise treat it as a directory and append "Shipit"
            if project_path.is_file() {
                project_path.to_path_buf()
            } else {
                project_path.join("Shipit")
            }
        }
    }
}

/// Normalize a path by resolving `.` and `..` components
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            std::path::Component::CurDir => {
                // Skip current directory references
            }
            std::path::Component::ParentDir => {
                // Pop the last component if possible
                components.pop();
            }
            _ => {
                components.push(component);
            }
        }
    }

    components.iter().collect()
}

/// Convert a path to use forward slashes (Unix-style)
pub fn to_forward_slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Join paths safely, normalizing the result
pub fn join_normalized(base: &Path, relative: &Path) -> PathBuf {
    normalize_path(&base.join(relative))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        let path = Path::new("./foo/./bar/../baz");
        let normalized = normalize_path(path);
        assert_eq!(normalized, PathBuf::from("foo/baz"));

        let path = Path::new("foo/bar/..");
        let normalized = normalize_path(path);
        assert_eq!(normalized, PathBuf::from("foo"));
    }

    #[test]
    fn test_to_forward_slashes() {
        let path = Path::new("foo/bar/baz");
        assert_eq!(to_forward_slashes(path), "foo/bar/baz");

        #[cfg(windows)]
        {
            let path = Path::new("foo\\bar\\baz");
            assert_eq!(to_forward_slashes(path), "foo/bar/baz");
        }
    }

    #[test]
    fn test_join_normalized() {
        let base = Path::new("/home/user");
        let relative = Path::new("./project/../other");
        let result = join_normalized(base, relative);
        assert_eq!(result, PathBuf::from("/home/user/other"));
    }

    #[test]
    fn test_resolve_shipit_path_file() {
        let path = Path::new("/tmp/project/Shipit");
        assert_eq!(
            resolve_shipit_path(path),
            PathBuf::from("/tmp/project/Shipit")
        );
    }

    #[test]
    fn test_resolve_shipit_path_directory() {
        let temp = tempfile::tempdir().expect("tempdir should be created");
        let resolved = resolve_shipit_path(temp.path());
        assert_eq!(resolved, temp.path().join("Shipit"));
    }

    #[test]
    fn test_resolve_shipit_path_with_override_none() {
        let project_path = Path::new("/home/user/project");
        let resolved = resolve_shipit_path_with_override(project_path, None);
        assert_eq!(resolved, PathBuf::from("/home/user/project/Shipit"));
    }

    #[test]
    fn test_resolve_shipit_path_with_override_file_path() {
        // Create a temporary file to test file path handling
        let temp = tempfile::tempdir().expect("tempdir should be created");
        let file_path = temp.path().join("CustomShipit");
        std::fs::write(&file_path, "test").expect("write should succeed");

        let resolved = resolve_shipit_path_with_override(&file_path, None);
        assert_eq!(resolved, file_path);
    }

    #[test]
    fn test_resolve_shipit_path_with_override_absolute() {
        let project_path = Path::new("/home/user/project");
        let override_path = Path::new("/tmp/custom/Shipit");
        let resolved = resolve_shipit_path_with_override(project_path, Some(override_path));
        assert_eq!(resolved, PathBuf::from("/tmp/custom/Shipit"));
    }

    #[test]
    fn test_resolve_shipit_path_with_override_relative() {
        let project_path = Path::new("/home/user/project");
        let override_path = Path::new("config/Shipit.custom");
        let resolved = resolve_shipit_path_with_override(project_path, Some(override_path));
        assert_eq!(
            resolved,
            PathBuf::from("/home/user/project/config/Shipit.custom")
        );
    }
}
