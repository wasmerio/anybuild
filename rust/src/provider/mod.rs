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

pub mod registry;
pub mod staticfile;
