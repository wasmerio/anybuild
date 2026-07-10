//! Shipit Starlark evaluation host.

pub mod config;
pub mod ctx;
pub mod eval;
pub mod loader;
pub mod values;

pub use ctx::Ctx;
pub use eval::{evaluate_shipit, EvaluateOptions};
