//! Provider system for detecting and building projects.

pub mod base;
pub mod go;
pub mod hugo;
pub mod laravel;
pub mod mkdocs;
pub mod node;
pub mod php;
pub mod python;
pub mod registry;
pub mod specs;
pub mod staticfile;

pub use base::Provider;
pub use registry::ProviderRegistry;
pub use specs::{
    DependencyKind, DependencySpec, DetectResult, MountSpec, ProviderPlan, ServiceSpec, VolumeSpec,
};
