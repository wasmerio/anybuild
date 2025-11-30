//! Providers encapsulate framework/language knowledge and produce plans.
//!
//! Each provider implements detection heuristics and returns a structured
//! `ProviderPlan` for generation.

use std::path::Path;

use crate::Result;
use crate::model::{CustomCommands, DetectResult, ProviderPlan};

/// Trait implemented by all providers.
pub trait Provider: Send + Sync {
    /// Provider name used in `serve(provider=...)`.
    fn name(&self) -> &'static str;

    /// Platform hint (e.g., `linux/amd64`) for Docker/Wasmer builds.
    fn platform(&self) -> Option<&str> {
        None
    }

    /// Run provider-specific initialization (e.g., cache detection results).
    fn initialize(&mut self) -> Result<()> {
        Ok(())
    }

    /// Returns a structured plan used by generator/CLI flows.
    fn plan(&self) -> Result<ProviderPlan>;
}

/// Registry entry describing how to detect and construct a provider.
pub trait ProviderDescriptor: Send + Sync {
    /// Attempt detection; higher scores win.
    fn detect(&self, path: &Path, custom: &CustomCommands) -> Option<DetectResult>;

    /// Construct an instance once selected.
    fn create(&self, path: &Path, custom: &CustomCommands) -> Result<Box<dyn Provider>>;

    /// Identifier for logging/registry ordering.
    fn name(&self) -> &'static str;
}

/// Apply custom command overrides to a commands map.
pub fn apply_custom_commands(
    custom: &CustomCommands,
    mut commands: std::collections::BTreeMap<String, String>,
) -> Result<std::collections::BTreeMap<String, String>> {
    if let Some(install) = &custom.install {
        commands.insert("install".to_string(), serde_json::to_string(install)?);
    }
    if let Some(build) = &custom.build {
        commands.insert("build".to_string(), serde_json::to_string(build)?);
    }
    if let Some(start) = &custom.start {
        commands.insert("start".to_string(), serde_json::to_string(start)?);
    }
    if let Some(after_deploy) = &custom.after_deploy {
        commands.insert(
            "after_deploy".to_string(),
            serde_json::to_string(after_deploy)?,
        );
    }
    Ok(commands)
}

pub mod hugo;
pub mod jekyll;
pub mod laravel;
pub mod mkdocs;
pub mod node_static;
pub mod php;
pub mod python;
pub mod registry;
pub mod staticfile;
pub mod wordpress;
