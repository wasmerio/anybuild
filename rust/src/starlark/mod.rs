//! Starlark DSL integration for Shipit files.

pub mod config;
pub mod ctx;
pub mod eval;
pub mod functions;

pub use config::ShipitConfig;
pub use ctx::Ctx;
pub use eval::{evaluate_shipit_file, with_ctx};
