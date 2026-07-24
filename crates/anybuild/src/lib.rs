//! Anybuild SDK.
//!
//! The [`Anybuild`] facade provides synchronous, typed access to generation,
//! planning, building, running, and deployment without going through the CLI.

mod error;
mod event;
mod internal;
mod sdk;

pub use error::{Error, ErrorKind, Result};
pub use event::{DiagnosticLevel, Event, EventHandler, ProcessIo, ProcessStream};
pub use sdk::{
    Anybuild, AutoOptions, AutoOutcome, BuildEnvironment, BuildOptions, BuildOutcome,
    CommandOverrides, DeployOptions, DeployOutcome, DeployTarget, DockerOptions, GenerateOptions,
    GeneratedAnybuild, GenerationPolicy, PlanOptions, ProjectPlan, RunOptions, RunOutcome,
    RuntimeEnvironment, WasmerOptions,
};

/// Stable plan types returned by [`Anybuild::plan`].
pub mod plan {
    pub use anybuild_plan::{
        CopyStep, EnvStep, Mount, Package, PathStep, RunStep, Serve, Service, Step, UseStep,
        Volume, WorkdirStep, WriteFileStep,
    };
}

/// Version of the Anybuild SDK and CLI.
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
