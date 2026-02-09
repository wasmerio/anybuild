//! Generator module for creating Shipit files from provider plans.

pub mod detect;
pub mod emit;

pub use detect::{detect_provider, load_provider};
pub use emit::generate_shipit_file;
