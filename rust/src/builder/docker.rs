use std::collections::BTreeMap;

use camino::Utf8PathBuf;

use crate::Result;
use crate::builder::Builder;
use crate::model::{Mount, PrepareStep, Serve, Step};

/// Docker builder emits Dockerfiles and builds images mirroring Python behavior.
pub struct DockerBuilder {
    pub src_dir: Utf8PathBuf,
    pub docker_client: String,
}

impl DockerBuilder {
    pub fn new(src_dir: Utf8PathBuf, docker_client: Option<String>) -> Self {
        Self {
            src_dir,
            docker_client: docker_client.unwrap_or_else(|| "docker".to_string()),
        }
    }
}

impl Builder for DockerBuilder {
    fn build(
        &mut self,
        _env: &BTreeMap<String, String>,
        _mounts: &[Mount],
        _steps: &[Step],
    ) -> Result<()> {
        todo!("DockerBuilder.build not yet implemented")
    }

    fn build_prepare(&mut self, _serve: &Serve) -> Result<()> {
        Ok(())
    }

    fn prepare(&mut self, _env: &BTreeMap<String, String>, _prepare: &[PrepareStep]) -> Result<()> {
        Ok(())
    }

    fn build_serve(&mut self, _serve: &Serve) -> Result<()> {
        todo!("DockerBuilder.build_serve not yet implemented")
    }

    fn finalize_build(&mut self, _serve: &Serve) -> Result<()> {
        todo!("DockerBuilder.finalize_build not yet implemented")
    }

    fn getenv(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn run_serve_command(&mut self, _command: &str) -> Result<()> {
        todo!("DockerBuilder.run_serve_command not yet implemented")
    }

    fn run_command(&mut self, _command: &str, _extra_args: Option<&[String]>) -> Result<()> {
        todo!("DockerBuilder.run_command not yet implemented")
    }

    fn get_build_mount_path(&self, name: &str) -> Utf8PathBuf {
        match name {
            "app" => Utf8PathBuf::from("/app"),
            other => Utf8PathBuf::from(format!("/opt/{}", other)),
        }
    }

    fn get_serve_mount_path(&self, name: &str) -> Utf8PathBuf {
        // Docker out directory layout mirrors build mount paths.
        let base = self
            .src_dir
            .join(".shipit/docker/out")
            .join(self.get_build_mount_path(name));
        base
    }
}
