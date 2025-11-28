//! Builders translate Shipit plans into runnable artifacts (local, Docker,
//! Wasmer).

use std::collections::BTreeMap;

use camino::Utf8PathBuf;

pub mod docker;
pub mod local;
pub mod wasmer;

use crate::Result;
use crate::model::{Mount, PrepareStep, Serve, Step};

/// Shared builder interface mirroring the Python implementation.
pub trait Builder: Send {
    /// Execute build steps in the target environment.
    fn build(
        &mut self,
        env: &BTreeMap<String, String>,
        mounts: &[Mount],
        steps: &[Step],
    ) -> Result<()>;

    /// Optional prepare phase to materialize artifacts prior to packaging.
    fn build_prepare(&mut self, serve: &Serve) -> Result<()>;

    /// Optional runtime prepare execution.
    fn prepare(&mut self, env: &BTreeMap<String, String>, prepare: &[PrepareStep]) -> Result<()>;

    /// Emit serve artifacts/scripts for the selected backend.
    fn build_serve(&mut self, serve: &Serve) -> Result<()>;

    /// Finalize build (e.g., write Dockerfile, build image).
    fn finalize_build(&mut self, serve: &Serve) -> Result<()>;

    /// Obtain an environment variable at build time.
    fn getenv(&self, name: &str) -> Option<String>;

    /// Run a serve command via the backend.
    fn run_serve_command(&mut self, command: &str) -> Result<()>;

    /// Run an arbitrary command with backend-specific handling.
    fn run_command(&mut self, command: &str, extra_args: Option<&[String]>) -> Result<()>;

    /// Map a mount name to build-path location for this backend.
    fn get_build_mount_path(&self, name: &str) -> Utf8PathBuf;

    /// Map a mount name to serve-path location for this backend.
    fn get_serve_mount_path(&self, name: &str) -> Utf8PathBuf;
}

/// Convenience type for build/serve environment maps.
pub type EnvMap = BTreeMap<String, String>;
