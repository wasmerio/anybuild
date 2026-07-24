//! Anybuild build backends.

use std::path::PathBuf;

use anyhow::Result;
use indexmap::IndexMap;

use anybuild_plan::{Mount, Step};

pub mod docker;
pub mod local;
pub mod ui;

/// Port of `builders/base.py::BuildBackend`.
pub trait BuildBackend {
    fn build(
        &mut self,
        name: &str,
        env: &IndexMap<String, String>,
        mounts: &[Mount],
        steps: &[Step],
    ) -> Result<()>;
    fn get_build_mount_path(&self, name: &str) -> PathBuf;
    fn get_artifact_mount_path(&self, name: &str) -> PathBuf;
    fn get_volume_path(&self, name: &str) -> PathBuf;
    fn get_runtime_path(&self) -> Option<String>;
}
