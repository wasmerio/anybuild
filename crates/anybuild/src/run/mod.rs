//! Runtime implementations.

use std::path::Path;

use anyhow::Result;
use indexmap::IndexMap;

use crate::plan::{RunStep, Serve, Step};
use crate::providers::ProviderConfig;
use crate::RuntimeArtifact;

pub mod docker;
pub mod lambda;
mod lambda_zip;
pub mod local;
pub mod wasmer;

pub(crate) struct HostMount<'a> {
    pub host_path: &'a Path,
    pub guest_path: &'a str,
}

/// Port of `runners/base.py::Runner`.
pub trait Runner {
    /// Apply runner-specific provider configuration before plan evaluation.
    fn prepare_config(&mut self, _config: &mut ProviderConfig) {}
    /// Retain the already resolved config for packaging metadata.
    fn record_provider_config(&mut self, _config: &ProviderConfig) {}
    fn prepare_build_steps(&self, steps: Vec<Step>) -> Vec<Step>;
    fn build(&mut self, serve: &Serve) -> Result<RuntimeArtifact>;
    fn prepare(&mut self, env: &IndexMap<String, String>, prepare: &[RunStep]) -> Result<()>;
    fn has_serve_command(&self, command: &str) -> bool;
    fn run_serve_command(
        &mut self,
        command: &str,
        volume_mappings: Option<&IndexMap<String, String>>,
        host_mounts: &[HostMount<'_>],
        env: Option<&IndexMap<String, String>>,
    ) -> Result<()>;
}
