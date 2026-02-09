//! Path utilities for cross-platform compatibility

use std::path::{Path, PathBuf};

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
}
