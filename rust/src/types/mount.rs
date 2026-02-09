//! Mount type definition

use allocative::Allocative;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A filesystem mount mapping build path to serve path
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Allocative)]
pub struct Mount {
    /// Name of the mount
    pub name: String,
    /// Path in the build environment
    pub build_path: PathBuf,
    /// Path in the serve environment
    pub serve_path: PathBuf,
}

impl Mount {
    /// Create a new mount
    pub fn new(
        name: impl Into<String>,
        build_path: impl Into<PathBuf>,
        serve_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            name: name.into(),
            build_path: build_path.into(),
            serve_path: serve_path.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mount_creation() {
        let mount = Mount::new("static", "/app/build", "/static");
        assert_eq!(mount.name, "static");
        assert_eq!(mount.build_path, PathBuf::from("/app/build"));
        assert_eq!(mount.serve_path, PathBuf::from("/static"));
    }

    #[test]
    fn test_mount_serialization() {
        let mount = Mount::new("public", "./public", "/public");
        let json = serde_json::to_string(&mount).unwrap();
        let deserialized: Mount = serde_json::from_str(&json).unwrap();
        assert_eq!(mount, deserialized);
    }
}
