//! Runtime execution (local and Wasmer)

pub mod base;
pub mod local;
pub mod wasmer;

pub use base::Runner;
pub use local::LocalRunner;
pub use wasmer::WasmerRunner;
