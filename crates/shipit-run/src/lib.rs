//! Shipit runners (port of `src/shipit/runners/`).

use std::path::PathBuf;

use anyhow::Result;
use indexmap::IndexMap;

use shipit_plan::{RunStep, Serve, Step};
use shipit_providers::ProviderConfig;

pub mod local;
pub mod wasmer;

/// Port of `runners/base.py::Runner`.
pub trait Runner {
    /// Apply the runner's config hook before evaluation. The wasmer runner
    /// splits plan-visible mutations from runner-only metadata (see
    /// `prepare_config` in runners/wasmer.py — an invariant, not an
    /// accident).
    fn prepare_config(&mut self, config: ProviderConfig) -> ProviderConfig;
    fn prepare_build_steps(&self, steps: Vec<Step>) -> Vec<Step>;
    fn build(&mut self, serve: &Serve) -> Result<()>;
    fn prepare(&mut self, env: &IndexMap<String, String>, prepare: &[RunStep]) -> Result<()>;
    fn has_serve_command(&self, command: &str) -> bool;
    fn run_serve_command(
        &mut self,
        command: &str,
        volume_mappings: Option<&IndexMap<String, String>>,
        env: Option<&IndexMap<String, String>>,
    ) -> Result<()>;
    fn get_serve_mount_path(&self, name: &str) -> PathBuf;
    /// Concrete-type escape hatch (Python's `assert isinstance(runner, ...)`
    /// in the deploy command).
    fn as_any(&mut self) -> &mut dyn std::any::Any;
}
