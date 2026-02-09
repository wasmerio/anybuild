//! Shipit CLI library.
//!
//! This library provides the core functionality for the Shipit CLI tool,
//! including provider detection, build planning, and project configuration.

pub mod builders;
pub mod cli;
pub mod config;
pub mod generator;
pub mod providers;
pub mod runners;
pub mod starlark;
pub mod types;
pub mod utils;

// Re-export commonly used items
pub use config::Config;
pub use providers::base::Provider;
pub use providers::registry::ProviderRegistry;
