//! Provider specification types

use crate::types::{Package, Service, Volume};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Detection result from a provider
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetectResult {
    /// Provider name (e.g., "python", "node-static")
    pub name: String,
    /// Confidence score (0.0 to 1.0, higher is better)
    pub confidence: f32,
    /// Reason for detection
    pub reason: String,
}

impl DetectResult {
    /// Create a new detection result
    pub fn new(name: impl Into<String>, confidence: f32, reason: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            confidence: confidence.clamp(0.0, 1.0),
            reason: reason.into(),
        }
    }

    /// Create with high confidence (0.9)
    pub fn high(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::new(name, 0.9, reason)
    }

    /// Create with medium confidence (0.6)
    pub fn medium(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::new(name, 0.6, reason)
    }

    /// Create with low confidence (0.3)
    pub fn low(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::new(name, 0.3, reason)
    }
}

/// Type of dependency
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DependencyKind {
    /// Runtime dependency (needed to run the app)
    Runtime,
    /// Build dependency (needed to build the app)
    Build,
    /// Development dependency (not needed in production)
    Dev,
}

/// Dependency specification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencySpec {
    /// Package name
    pub name: String,
    /// Variable name in Shipit plan
    #[serde(skip_serializing_if = "Option::is_none")]
    pub var_name: Option<String>,
    /// Default version if not specified
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_version: Option<String>,
    /// Variable name for architecture
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architecture_var_name: Option<String>,
    /// Alias for the dependency
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    /// Use in build phase
    #[serde(default)]
    pub use_in_build: bool,
    /// Use in serve phase
    #[serde(default)]
    pub use_in_serve: bool,
    /// Dependency kind
    #[serde(default = "default_dependency_kind")]
    pub kind: DependencyKind,
}

fn default_dependency_kind() -> DependencyKind {
    DependencyKind::Runtime
}

impl DependencySpec {
    /// Create a new dependency spec
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            var_name: None,
            default_version: None,
            architecture_var_name: None,
            alias: None,
            use_in_build: false,
            use_in_serve: false,
            kind: DependencyKind::Runtime,
        }
    }

    /// Convert to Package type
    pub fn to_package(&self) -> Package {
        Package {
            name: self.name.clone(),
            version: self.default_version.clone(),
            architecture: None,
        }
    }
}

/// Mount specification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountSpec {
    /// Mount name
    pub name: String,
    /// Attach to build phase
    #[serde(default = "default_true")]
    pub attach_to_build: bool,
    /// Attach to serve phase
    #[serde(default = "default_true")]
    pub attach_to_serve: bool,
}

fn default_true() -> bool {
    true
}

impl MountSpec {
    /// Create a new mount spec
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            attach_to_build: true,
            attach_to_serve: true,
        }
    }

    /// Only attach to build
    pub fn build_only(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            attach_to_build: true,
            attach_to_serve: false,
        }
    }

    /// Only attach to serve
    pub fn serve_only(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            attach_to_build: false,
            attach_to_serve: true,
        }
    }
}

/// Volume specification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeSpec {
    /// Volume name
    pub name: String,
    /// Absolute path in serve environment
    pub serve_path: PathBuf,
    /// Variable name in Shipit plan
    #[serde(skip_serializing_if = "Option::is_none")]
    pub var_name: Option<String>,
}

impl VolumeSpec {
    /// Create a new volume spec
    pub fn new(name: impl Into<String>, serve_path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            serve_path: serve_path.into(),
            var_name: None,
        }
    }

    /// Convert to Volume type
    pub fn to_volume(&self) -> Volume {
        Volume {
            name: self.name.clone(),
            serve_path: self.serve_path.clone(),
        }
    }
}

/// Service specification (wraps the Service type from types module)
pub type ServiceSpec = Service;

/// Complete provider plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPlan {
    /// Name for the serve configuration
    pub serve_name: String,
    /// Provider name
    pub provider: String,
    /// Mounts
    pub mounts: Vec<MountSpec>,
    /// Platform (e.g., "wasmer/python")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    /// Volumes
    #[serde(default)]
    pub volumes: Vec<VolumeSpec>,
    /// Declarations (Starlark code at top of file)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declarations: Option<String>,
    /// Dependencies
    #[serde(default)]
    pub dependencies: Vec<DependencySpec>,
    /// Build steps (Starlark function calls)
    #[serde(default)]
    pub build_steps: Vec<String>,
    /// Prepare steps (run before serve)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prepare: Option<Vec<String>>,
    /// Services (databases, etc.)
    #[serde(default)]
    pub services: Vec<ServiceSpec>,
    /// Commands (e.g., web, worker)
    #[serde(default)]
    pub commands: HashMap<String, String>,
    /// Environment variables
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
}

impl ProviderPlan {
    /// Create a new provider plan
    pub fn new(serve_name: impl Into<String>, provider: impl Into<String>) -> Self {
        Self {
            serve_name: serve_name.into(),
            provider: provider.into(),
            mounts: Vec::new(),
            platform: None,
            volumes: Vec::new(),
            declarations: None,
            dependencies: Vec::new(),
            build_steps: Vec::new(),
            prepare: None,
            services: Vec::new(),
            commands: HashMap::new(),
            env: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_result_confidence_clamping() {
        let result = DetectResult::new("test", 1.5, "reason");
        assert_eq!(result.confidence, 1.0);

        let result = DetectResult::new("test", -0.5, "reason");
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn test_detect_result_helpers() {
        let high = DetectResult::high("python", "found pyproject.toml");
        assert_eq!(high.confidence, 0.9);

        let medium = DetectResult::medium("node", "found package.json");
        assert_eq!(medium.confidence, 0.6);

        let low = DetectResult::low("static", "no specific markers");
        assert_eq!(low.confidence, 0.3);
    }

    #[test]
    fn test_dependency_spec() {
        let dep = DependencySpec::new("python");
        assert_eq!(dep.name, "python");
        assert_eq!(dep.kind, DependencyKind::Runtime);
    }

    #[test]
    fn test_mount_spec_helpers() {
        let mount = MountSpec::build_only("cache");
        assert!(mount.attach_to_build);
        assert!(!mount.attach_to_serve);

        let mount = MountSpec::serve_only("public");
        assert!(!mount.attach_to_build);
        assert!(mount.attach_to_serve);
    }

    #[test]
    fn test_volume_spec() {
        let vol = VolumeSpec::new("data", "/var/data");
        let volume = vol.to_volume();
        assert_eq!(volume.name, "data");
        assert_eq!(volume.serve_path, PathBuf::from("/var/data"));
    }

    #[test]
    fn test_provider_plan() {
        let plan = ProviderPlan::new("my-app", "python");
        assert_eq!(plan.serve_name, "my-app");
        assert_eq!(plan.provider, "python");
        assert!(plan.mounts.is_empty());
        assert!(plan.dependencies.is_empty());
    }
}
