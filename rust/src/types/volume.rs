//! Volume type definition

use allocative::Allocative;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A persistent volume
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Allocative)]
pub struct Volume {
    /// Name of the volume
    pub name: String,
    /// Path in the serve environment
    pub serve_path: PathBuf,
}

impl Volume {
    /// Create a new volume
    pub fn new(name: impl Into<String>, serve_path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            serve_path: serve_path.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volume_creation() {
        let volume = Volume::new("data", "/data");
        assert_eq!(volume.name, "data");
        assert_eq!(volume.serve_path, PathBuf::from("/data"));
    }

    #[test]
    fn test_volume_serialization() {
        let volume = Volume::new("uploads", "/var/uploads");
        let json = serde_json::to_string(&volume).unwrap();
        let deserialized: Volume = serde_json::from_str(&json).unwrap();
        assert_eq!(volume, deserialized);
    }
}
