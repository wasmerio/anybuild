//! Shipit Rust rewrite library entrypoint.
//!
//! The crate exposes modules for the CLI, provider detection, Starlark
//! evaluation, builders, and plan generation.

pub mod assets;
pub mod builder;
pub mod cli;
pub mod context;
pub mod detect;
pub mod env;
pub mod generator;
pub mod model;
pub mod procfile;
pub mod provider;
pub mod starlark_ast;
pub mod starlark_runtime;
pub mod util;

/// Common result type using `anyhow` for error handling during rapid
/// prototyping. We may introduce richer error types as the implementation
/// solidifies.
pub type Result<T> = anyhow::Result<T>;
