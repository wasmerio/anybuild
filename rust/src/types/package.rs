//! Package type definition

use allocative::Allocative;
use serde::{Deserialize, Serialize};
use std::fmt;

/// CPU architecture for a package
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Allocative)]
pub enum Architecture {
    #[serde(rename = "64-bit")]
    Bit64,
    #[serde(rename = "32-bit")]
    Bit32,
}

/// A package dependency
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Allocative)]
pub struct Package {
    /// Package name
    pub name: String,
    /// Optional version specification
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Optional architecture specification
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architecture: Option<Architecture>,
}

impl Package {
    /// Create a new package with just a name
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: None,
            architecture: None,
        }
    }

    /// Create a package with a version
    pub fn with_version(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: Some(version.into()),
            architecture: None,
        }
    }

    /// Create a package with architecture
    pub fn with_architecture(name: impl Into<String>, arch: Architecture) -> Self {
        Self {
            name: name.into(),
            version: None,
            architecture: Some(arch),
        }
    }

    /// Set the version
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Set the architecture
    pub fn architecture(mut self, arch: Architecture) -> Self {
        self.architecture = Some(arch);
        self
    }
}

impl fmt::Display for Package {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = if let Some(arch) = self.architecture {
            format!("{}({:?})", self.name, arch)
        } else {
            self.name.clone()
        };

        if let Some(version) = &self.version {
            write!(f, "{}@{}", name, version)
        } else {
            write!(f, "{}", name)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_display() {
        let pkg = Package::new("python");
        assert_eq!(pkg.to_string(), "python");

        let pkg = Package::with_version("python", "3.11");
        assert_eq!(pkg.to_string(), "python@3.11");

        let pkg = Package::with_architecture("python", Architecture::Bit64);
        assert_eq!(pkg.to_string(), "python(Bit64)");

        let pkg = Package::new("python")
            .version("3.11")
            .architecture(Architecture::Bit64);
        assert_eq!(pkg.to_string(), "python(Bit64)@3.11");
    }

    #[test]
    fn test_package_builders() {
        let pkg = Package::new("node");
        assert_eq!(pkg.name, "node");
        assert!(pkg.version.is_none());
        assert!(pkg.architecture.is_none());

        let pkg = pkg.version("20.0.0");
        assert_eq!(pkg.version, Some("20.0.0".to_string()));
    }

    #[test]
    fn test_package_serialization() {
        let pkg = Package::new("python").version("3.11");
        let json = serde_json::to_string(&pkg).unwrap();
        let deserialized: Package = serde_json::from_str(&json).unwrap();
        assert_eq!(pkg, deserialized);
    }
}
