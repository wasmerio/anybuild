//! Core type definitions for Shipit

pub mod mount;
pub mod package;
pub mod serve;
pub mod service;
pub mod steps;
pub mod volume;

pub use mount::Mount;
pub use package::Package;
pub use service::{Service, ServiceProvider};
pub use steps::{CopyStep, EnvStep, PathStep, PrepareStep, RunStep, Step, UseStep, WorkdirStep};
pub use volume::Volume;
