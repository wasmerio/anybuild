//! Anybuild SDK.
//!
//! The [`Anybuild`] facade provides synchronous, typed access to generation,
//! planning, building, running, and deployment without going through the CLI.

mod artifact;
mod build;
mod common;
mod deploy;
mod error;
mod event;
mod internal;
mod operation;
pub mod plan;
mod providers;
mod run;
mod sdk;
mod starlark;
mod wasmer;

pub use artifact::{ArtifactKind, RuntimeArtifact};
pub use error::{Error, ErrorKind, Result};
pub use event::{
    BuildPlanPackage, BuildPlanStep, DeployScript, DiagnosticLevel, Event, EventHandler,
    PackagePhase, ProcessIo, ProcessStream, ProviderDetail, WasmerPackageMapping,
};
pub use sdk::{
    Anybuild, AutoOptions, AutoOutcome, AwsLambdaOptions, BuildEnvironment, BuildOptions,
    BuildOutcome, CommandOverrides, ConfigDifference, DeployOptions, DeployOutcome, DeployTarget,
    DeploymentPlatform, DockerOptions, FlyOptions, GenerateOptions, GeneratedAnybuild,
    GenerationCheck, GenerationCheckStatus, GenerationPolicy, LambdaArchitecture, PlanOptions,
    ProjectPlan, ProviderConfigSnapshot, RunOptions, RunOutcome, RuntimeEnvironment, WasmerOptions,
};

/// Version of the Anybuild SDK and CLI.
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
#[path = "tests/config_differential.rs"]
mod config_differential;
#[cfg(test)]
#[path = "tests/plan_serde_contract.rs"]
mod plan_serde_contract;
#[cfg(test)]
#[path = "tests/starlark_evaluator.rs"]
mod starlark_evaluator;
#[cfg(test)]
#[path = "tests/starlark_loader.rs"]
mod starlark_loader;
#[cfg(test)]
#[path = "tests/starlark_snapshots.rs"]
mod starlark_snapshots;
