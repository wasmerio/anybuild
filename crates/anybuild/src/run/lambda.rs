//! AWS Lambda runtime packaging.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{bail, Result};
use indexmap::IndexMap;

use crate::build::BuildBackend;
use crate::operation::OperationContext;
use crate::plan::{RunStep, Serve, Step};
use crate::providers::ProviderConfig;
use crate::run::docker::DockerRunner;
use crate::run::{lambda_zip, HostMount, Runner};
use crate::RuntimeArtifact;

pub struct LambdaRunner {
    build_backend: Rc<RefCell<dyn BuildBackend>>,
    anybuild_dir: PathBuf,
    runner_path: PathBuf,
    docker: DockerRunner,
    operation: OperationContext,
}

impl LambdaRunner {
    pub fn new(
        build_backend: Rc<RefCell<dyn BuildBackend>>,
        src_dir: PathBuf,
        docker_client: Option<String>,
        docker_opts: Option<String>,
        anybuild_dir: Option<PathBuf>,
        operation: OperationContext,
    ) -> Self {
        let anybuild_dir = anybuild_dir.unwrap_or_else(|| src_dir.join(".anybuild"));
        let runner_path = anybuild_dir.join("runner").join("lambda");
        let docker = DockerRunner::new(
            build_backend.clone(),
            src_dir,
            docker_client,
            docker_opts,
            Some(anybuild_dir.clone()),
            operation.clone(),
        );
        Self {
            build_backend,
            anybuild_dir,
            runner_path,
            docker,
            operation,
        }
    }

    fn docker_artifact_available(&self) -> bool {
        self.anybuild_dir.join("runner/docker/name").is_file()
    }

    fn clear_path(path: &std::path::Path) -> Result<()> {
        match std::fs::remove_dir_all(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

impl Runner for LambdaRunner {
    fn prepare_config(&mut self, config: &mut ProviderConfig) {
        self.docker.prepare_config(config);
    }

    fn record_provider_config(&mut self, config: &ProviderConfig) {
        self.docker.record_provider_config(config);
    }

    fn prepare_build_steps(&self, steps: Vec<Step>) -> Vec<Step> {
        self.docker.prepare_build_steps(steps)
    }

    fn build(&mut self, serve: &Serve) -> Result<RuntimeArtifact> {
        Self::clear_path(&self.runner_path)?;
        let archive = self.runner_path.join("function.zip");
        let artifact = lambda_zip::package(serve, &*self.build_backend.borrow(), &archive)?;
        if let Some(artifact) = artifact {
            Self::clear_path(&self.anybuild_dir.join("runner/docker"))?;
            let RuntimeArtifact::LambdaZip {
                archive, runtime, ..
            } = &artifact
            else {
                unreachable!("Lambda ZIP packager returned a different artifact")
            };
            crate::build::report::success(
                &self.operation,
                format!(
                    "Created AWS Lambda {} archive {}",
                    runtime,
                    archive.display()
                ),
            );
            return Ok(artifact);
        }
        self.docker.build(serve)
    }

    fn prepare(&mut self, env: &IndexMap<String, String>, prepare: &[RunStep]) -> Result<()> {
        if self.docker_artifact_available() {
            self.docker.prepare(env, prepare)
        } else {
            Ok(())
        }
    }

    fn has_serve_command(&self, command: &str) -> bool {
        self.docker_artifact_available() && self.docker.has_serve_command(command)
    }

    fn run_serve_command(
        &mut self,
        command: &str,
        volume_mappings: Option<&IndexMap<String, String>>,
        host_mounts: &[HostMount<'_>],
        env: Option<&IndexMap<String, String>>,
    ) -> Result<()> {
        if !self.docker_artifact_available() {
            bail!(
                "Lambda ZIP artifacts cannot run locally; rebuild with --runner=docker to run this service"
            );
        }
        self.docker
            .run_serve_command(command, volume_mappings, host_mounts, env)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{Mount, Package};

    struct TestBackend {
        root: PathBuf,
    }

    impl BuildBackend for TestBackend {
        fn build(
            &mut self,
            _name: &str,
            _env: &IndexMap<String, String>,
            _mounts: &[Mount],
            _steps: &[Step],
        ) -> Result<()> {
            Ok(())
        }

        fn get_build_mount_path(&self, name: &str) -> PathBuf {
            self.root.join(name)
        }

        fn get_artifact_mount_path(&self, name: &str) -> PathBuf {
            self.root.join(name)
        }

        fn get_volume_path(&self, name: &str) -> PathBuf {
            self.root.join(name)
        }

        fn get_runtime_path(&self) -> Option<String> {
            None
        }

        fn artifact_platform(&self) -> Option<&str> {
            Some("linux/amd64")
        }
    }

    #[test]
    fn managed_runtime_build_produces_only_a_lambda_zip() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("source");
        let artifacts = temporary.path().join("artifacts");
        let anybuild_dir = source.join(".anybuild");
        std::fs::create_dir_all(artifacts.join("app")).unwrap();
        std::fs::write(artifacts.join("app/server.js"), "console.log('ready')\n").unwrap();
        let backend: Rc<RefCell<dyn BuildBackend>> =
            Rc::new(RefCell::new(TestBackend { root: artifacts }));
        let mut runner = LambdaRunner::new(
            backend,
            source,
            None,
            None,
            Some(anybuild_dir.clone()),
            OperationContext::for_test(),
        );
        let serve = Serve {
            name: "api".to_owned(),
            provider: "node".to_owned(),
            runtime_port: Some(8080),
            build: Vec::new(),
            deps: vec![Package {
                name: "node".to_owned(),
                version: Some("24".to_owned()),
                architecture: None,
            }],
            commands: IndexMap::from([("start".to_owned(), "node server.js".to_owned())]),
            cwd: Some("/app".to_owned()),
            prepare: None,
            mounts: Some(vec![Mount {
                name: "app".to_owned(),
                build_path: PathBuf::new(),
                serve_path: PathBuf::from("/app"),
            }]),
            volumes: None,
            env: None,
            services: None,
        };

        let artifact = runner.build(&serve).unwrap();

        assert_eq!(artifact.kind(), crate::ArtifactKind::LambdaZip);
        assert!(anybuild_dir.join("runner/lambda/function.zip").is_file());
        assert!(!anybuild_dir.join("runner/docker").exists());
    }
}
