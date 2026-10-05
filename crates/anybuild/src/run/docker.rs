//! Docker runtime packaging and execution.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

use anyhow::{bail, ensure, Context, Result};
use indexmap::IndexMap;

use crate::build::docker::dependency_install_contents;
use crate::build::BuildBackend;
use crate::internal::volumes::load_volume_mappings;
use crate::operation::OperationContext;
use crate::plan::{RunStep, Serve, Step};
use crate::run::{HostMount, Runner};
use crate::RuntimeArtifact;

const LAMBDA_ADAPTER_IMAGE: &str = "public.ecr.aws/awsguru/aws-lambda-adapter:1.0.0";

const TOOLCHAIN_STAGE: &str = r#"# syntax=docker/dockerfile:1.7-labs
FROM debian:trixie-slim AS runtime-tools

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt/lists,sharing=locked \
    rm -f /etc/apt/apt.conf.d/docker-clean \
    && apt-get update \
    && apt-get -y --no-install-recommends install \
        curl ca-certificates unzip git xz-utils

SHELL ["/bin/bash", "-o", "pipefail", "-c"]
"#;

const MISE_SETUP: &str = r#"ENV MISE_DATA_DIR="/mise"
ENV MISE_CONFIG_DIR="/mise"
ENV MISE_CACHE_DIR="/mise/cache"
ENV MISE_INSTALL_PATH="/usr/local/bin/mise"
ENV PATH="/mise/shims:$PATH"

RUN curl https://mise.run | sh
"#;

const RUNTIME_STAGE: &str = r#"
FROM debian:trixie-slim AS runtime

SHELL ["/bin/bash", "-o", "pipefail", "-c"]
ENV MISE_DATA_DIR="/mise"
ENV MISE_CONFIG_DIR="/mise"
ENV PATH="/mise/shims:$PATH"

COPY --from=runtime-tools /etc/ssl/certs /etc/ssl/certs
"#;

pub struct DockerRunner {
    build_backend: Rc<RefCell<dyn BuildBackend>>,
    src_dir: PathBuf,
    anybuild_dir: PathBuf,
    runner_path: PathBuf,
    bin_path: PathBuf,
    dockerfile_path: PathBuf,
    dockerignore_path: PathBuf,
    image_name_path: PathBuf,
    port_path: PathBuf,
    docker_client: String,
    docker_opts: Option<String>,
    operation: OperationContext,
}

impl DockerRunner {
    pub fn new(
        build_backend: Rc<RefCell<dyn BuildBackend>>,
        src_dir: PathBuf,
        docker_client: Option<String>,
        docker_opts: Option<String>,
        anybuild_dir: Option<PathBuf>,
        operation: OperationContext,
    ) -> Self {
        let anybuild_dir = anybuild_dir.unwrap_or_else(|| src_dir.join(".anybuild"));
        let runner_path = anybuild_dir.join("runner").join("docker");
        Self {
            build_backend,
            src_dir,
            anybuild_dir,
            bin_path: runner_path.join("bin"),
            dockerfile_path: runner_path.join("Dockerfile"),
            dockerignore_path: runner_path.join("Dockerfile.dockerignore"),
            image_name_path: runner_path.join("name"),
            port_path: runner_path.join("port"),
            runner_path,
            docker_client: docker_client.unwrap_or_else(|| "docker".to_owned()),
            docker_opts,
            operation,
        }
    }

    fn image_name(serve_name: &str) -> String {
        crate::build::docker::internal_image_name(serve_name)
    }

    fn context_path(&self, path: &Path) -> Result<String> {
        let path = path.strip_prefix(&self.src_dir).with_context(|| {
            format!(
                "Docker runner artifact {} is outside build context {}",
                path.display(),
                self.src_dir.display()
            )
        })?;
        Ok(path.to_string_lossy().replace('\\', "/"))
    }

    fn write_script(&self, name: &str, cwd: Option<&str>, body: &str) -> Result<()> {
        let mut contents = String::from("#!/bin/bash\nset -e\n");
        if let Some(cwd) = cwd {
            contents.push_str(&format!("cd {}\n", shell_quote(cwd)));
        }
        contents.push_str(body);
        contents.push('\n');
        let path = self.bin_path.join(name);
        std::fs::write(&path, contents)?;
        set_executable(&path)
    }

    fn write_scripts(&self, serve: &Serve) -> Result<()> {
        std::fs::create_dir_all(&self.bin_path)?;
        for (name, body) in &serve.commands {
            self.write_script(name, serve.cwd.as_deref(), body)?;
        }
        if let Some(prepare) = &serve.prepare {
            if !prepare.is_empty() {
                let body = prepare
                    .iter()
                    .map(|step| step.command.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                self.write_script("prepare", serve.cwd.as_deref(), &body)?;
            }
        }
        let entrypoint = self.bin_path.join("entrypoint");
        std::fs::write(
            &entrypoint,
            "#!/bin/bash\nset -e\ncommand_name=${1:-start}\nshift || true\nif [ -x \"/anybuild/bin/$command_name\" ]; then\n  exec \"/anybuild/bin/$command_name\" \"$@\"\nfi\nexec \"$command_name\" \"$@\"\n",
        )?;
        set_executable(&entrypoint)
    }

    fn dockerfile_contents(&self, serve: &Serve) -> Result<String> {
        let adapter_image = self
            .operation
            .environment_var("ANYBUILD_LAMBDA_ADAPTER_IMAGE")
            .filter(|image| !image.is_empty())
            .unwrap_or_else(|| LAMBDA_ADAPTER_IMAGE.to_owned());
        ensure!(
            !adapter_image.chars().any(char::is_whitespace),
            "ANYBUILD_LAMBDA_ADAPTER_IMAGE must be an image reference without whitespace"
        );
        let mut contents = String::from(TOOLCHAIN_STAGE);
        let uses_mise = serve.deps.iter().any(|dependency| {
            !matches!(
                dependency.name.as_str(),
                "bash" | "composer" | "pie" | "sendmail" | "static-web-server"
            )
        });
        if uses_mise {
            contents.push_str(MISE_SETUP);
        }
        for dependency in &serve.deps {
            if dependency.name != "sendmail" {
                contents.push_str(&dependency_install_contents(dependency));
            }
        }
        contents.push_str(RUNTIME_STAGE);
        // Sendmail needs its distro configuration and libraries in the final
        // image, rather than a binary copied out of the toolchain stage.
        for dependency in &serve.deps {
            if dependency.name == "sendmail" {
                contents.push_str(&dependency_install_contents(dependency));
            }
        }
        contents.push_str(&format!(
            "COPY --from={adapter_image} /lambda-adapter /opt/extensions/lambda-adapter\n"
        ));
        if uses_mise {
            contents.push_str(
                "# Copy resolved toolchains and shared libraries without compilers or caches.\n",
            );
            contents.push_str("COPY --from=runtime-tools /mise /mise\n");
            contents.push_str("COPY --from=runtime-tools /usr/local/bin /usr/local/bin\n");
        } else if serve
            .deps
            .iter()
            .any(|dependency| dependency.name == "static-web-server")
        {
            contents.push_str(
                "COPY --from=runtime-tools /usr/local/bin/static-web-server /usr/local/bin/static-web-server\n",
            );
        }
        if serve
            .deps
            .iter()
            .any(|dependency| dependency.name == "composer")
        {
            contents.push_str("COPY --from=runtime-tools /usr/bin/composer /usr/bin/composer\n");
        }
        if serve.deps.iter().any(|dependency| dependency.name == "pie") {
            contents.push_str("COPY --from=runtime-tools /usr/bin/pie /usr/bin/pie\n");
        }

        for mount in serve.mounts.as_deref().unwrap_or_default() {
            let source = self
                .build_backend
                .borrow()
                .get_artifact_mount_path(&mount.name);
            let source = self.context_path(&source)?;
            contents.push_str(&format!(
                "COPY {}\n",
                serde_json::to_string(&(source, mount.serve_path.to_string_lossy()))?
            ));
        }

        let bin_path = self.context_path(&self.bin_path)?;
        contents.push_str(&format!(
            "COPY {}\nRUN chmod +x /anybuild/bin/*\n",
            serde_json::to_string(&(bin_path, "/anybuild/bin"))?
        ));
        if serve
            .mounts
            .as_deref()
            .unwrap_or_default()
            .iter()
            .any(|mount| mount.serve_path == Path::new("/opt/venv"))
        {
            contents.push_str("ENV PATH=\"/opt/venv/bin:$PATH\"\n");
        }
        for (key, value) in serve.env.as_ref().into_iter().flatten() {
            contents.push_str(&format!("ENV {key}={}\n", docker_env_value(value)));
        }
        if !serve
            .env
            .as_ref()
            .is_some_and(|env| env.contains_key("HOST"))
        {
            contents.push_str("ENV HOST=\"0.0.0.0\"\n");
        }
        contents.push_str(&format!(
            "ENV AWS_LWA_PORT={}\n",
            serve.runtime_port.unwrap_or(8080)
        ));
        if let Some(cwd) = &serve.cwd {
            contents.push_str(&format!("WORKDIR {}\n", docker_env_value(cwd)));
        }
        contents.push_str("ENTRYPOINT [\"/anybuild/bin/entrypoint\"]\n");
        if let Some(port) = serve.runtime_port {
            contents.push_str(&format!("EXPOSE {port}\n"));
        }
        contents.push_str("CMD [\"start\"]\n");
        Ok(contents)
    }

    fn write_dockerignore(&self, serve: &Serve) -> Result<()> {
        let mut included = vec![self.context_path(&self.bin_path)?];
        for mount in serve.mounts.as_deref().unwrap_or_default() {
            included.push(
                self.context_path(
                    &self
                        .build_backend
                        .borrow()
                        .get_artifact_mount_path(&mount.name),
                )?,
            );
        }
        let mut contents = String::from("**\n");
        for path in included {
            let mut current = PathBuf::new();
            for component in Path::new(&path).components() {
                current.push(component);
                contents.push_str(&format!("!{}/\n", current.to_string_lossy()));
            }
            contents.push_str(&format!("!{path}/**\n"));
        }
        std::fs::write(&self.dockerignore_path, contents)?;
        Ok(())
    }

    fn build_image(&self, image_name: &str) -> Result<()> {
        let mut command = Command::new(&self.docker_client);
        self.operation.prepare_command(&mut command);
        command
            .arg("build")
            .arg("-f")
            .arg(&self.dockerfile_path)
            .arg("-t")
            .arg(image_name);
        if let Some(platform) = self.build_backend.borrow().artifact_platform() {
            command.arg("--platform").arg(platform);
        }
        if let Some(options) = &self.docker_opts {
            command.arg(options);
        }
        command.arg(&self.src_dir);
        let status = self
            .operation
            .command_status(&mut command)
            .with_context(|| format!("failed to run {}", self.docker_client))?;
        ensure!(
            status.success(),
            "Command {} build failed with exit code {:?}",
            self.docker_client,
            status.code()
        );
        Ok(())
    }

    fn stored_image_name(&self) -> Result<String> {
        Ok(std::fs::read_to_string(&self.image_name_path)
            .with_context(|| {
                format!(
                    "Docker image metadata is missing; build with --runner=docker first ({})",
                    self.image_name_path.display()
                )
            })?
            .trim()
            .to_owned())
    }

    fn volume_args(&self, mappings: &IndexMap<String, String>) -> Result<Vec<String>> {
        let mut args = Vec::new();
        for (name, guest_path) in mappings {
            let host_path = std::path::absolute(self.anybuild_dir.join("volumes").join(name))?;
            args.push("--volume".to_owned());
            args.push(format!("{}:{guest_path}", host_path.display()));
        }
        Ok(args)
    }
}

impl Runner for DockerRunner {
    fn prepare_build_steps(&self, steps: Vec<Step>) -> Vec<Step> {
        steps
    }

    fn build(&mut self, serve: &Serve) -> Result<RuntimeArtifact> {
        match std::fs::remove_dir_all(&self.runner_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        self.write_scripts(serve)?;
        let dockerfile = self.dockerfile_contents(serve)?;
        std::fs::write(&self.dockerfile_path, &dockerfile)?;
        self.write_dockerignore(serve)?;
        let image_name = Self::image_name(&serve.name);
        std::fs::write(&self.image_name_path, &image_name)?;
        std::fs::write(
            &self.port_path,
            serve.runtime_port.unwrap_or(8080).to_string(),
        )?;
        crate::build::report::section_started(&self.operation, "Packaging Docker image");
        crate::build::report::print_syntax_panel(&self.operation, &dockerfile, "dockerfile");
        self.build_image(&image_name)?;
        crate::build::report::success(
            &self.operation,
            format!("Created Docker image {image_name}"),
        );
        let platform = self
            .build_backend
            .borrow()
            .artifact_platform()
            .map(str::to_owned);
        Ok(RuntimeArtifact::Docker {
            directory: self.runner_path.clone(),
            image: image_name,
            context: self.src_dir.clone(),
            platform,
        })
    }

    fn prepare(&mut self, env: &IndexMap<String, String>, prepare: &[RunStep]) -> Result<()> {
        if prepare.is_empty() {
            return Ok(());
        }
        let mappings = load_volume_mappings(&self.src_dir, Some(&self.anybuild_dir))?;
        self.run_serve_command("prepare", Some(&mappings), &[], Some(env))
    }

    fn has_serve_command(&self, command: &str) -> bool {
        self.bin_path.join(command).is_file()
    }

    fn run_serve_command(
        &mut self,
        command: &str,
        volume_mappings: Option<&IndexMap<String, String>>,
        host_mounts: &[HostMount<'_>],
        env: Option<&IndexMap<String, String>>,
    ) -> Result<()> {
        let parsed = shlex::split(command).unwrap_or_default();
        if parsed.is_empty() {
            bail!("Serve command cannot be empty");
        }
        let image_name = self.stored_image_name()?;
        let mut args = vec!["run".to_owned(), "--rm".to_owned()];
        args.extend([
            "--add-host".to_owned(),
            "host.docker.internal:host-gateway".to_owned(),
        ]);
        if parsed[0] == "start" {
            let host_port = env
                .and_then(|values| values.get("PORT"))
                .map(String::as_str)
                .unwrap_or("8080");
            let container_port =
                std::fs::read_to_string(&self.port_path).unwrap_or_else(|_| "8080".to_owned());
            args.extend([
                "--publish".to_owned(),
                format!("{host_port}:{}", container_port.trim()),
            ]);
        }
        if let Some(mappings) = volume_mappings {
            args.extend(self.volume_args(mappings)?);
        }
        for mount in host_mounts {
            args.push("--volume".to_owned());
            args.push(format!(
                "{}:{}",
                std::path::absolute(mount.host_path)?.display(),
                mount.guest_path
            ));
        }
        if let Some(env) = env {
            for (key, value) in env {
                let value = docker_container_env_value(key, value);
                args.extend(["--env".to_owned(), format!("{key}={value}")]);
            }
        }
        args.push(image_name);
        args.extend(parsed);

        let mut process = Command::new(&self.docker_client);
        self.operation.prepare_command(&mut process);
        process.args(args);
        let status = self
            .operation
            .command_status(&mut process)
            .with_context(|| format!("failed to run {}", self.docker_client))?;
        ensure!(
            status.success(),
            "Command {} run failed with exit code {:?}",
            self.docker_client,
            status.code()
        );
        Ok(())
    }
}

fn docker_container_env_value<'a>(key: &str, value: &'a str) -> &'a str {
    if matches!(key, "DB_HOST" | "DATABASE_HOST") && matches!(value, "127.0.0.1" | "localhost") {
        "host.docker.internal"
    } else {
        value
    }
}

fn docker_env_value(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
    )
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn set_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::local::LocalBuildBackend;
    use crate::event::{ProcessIo, Reporter};
    use crate::plan::{Mount, Package};

    fn runner(root: &Path) -> DockerRunner {
        runner_with_env(root, IndexMap::new())
    }

    fn runner_with_env(root: &Path, environment: IndexMap<String, String>) -> DockerRunner {
        let operation =
            OperationContext::new(environment, false, ProcessIo::Inherit, Reporter::default());
        let source = root.join("src");
        let anybuild_dir = source.join(".anybuild");
        std::fs::create_dir_all(&source).unwrap();
        let backend: Rc<RefCell<dyn BuildBackend>> = Rc::new(RefCell::new(LocalBuildBackend::new(
            source.clone(),
            root.join("assets"),
            Some(anybuild_dir.clone()),
            operation.clone(),
        )));
        std::fs::create_dir_all(backend.borrow().get_artifact_mount_path("app")).unwrap();
        DockerRunner::new(backend, source, None, None, Some(anybuild_dir), operation)
    }

    fn serve() -> Serve {
        Serve {
            name: "Acme Web".to_owned(),
            provider: "node".to_owned(),
            runtime_port: Some(8080),
            build: Vec::new(),
            deps: vec![Package {
                name: "node".to_owned(),
                version: Some("22".to_owned()),
                architecture: None,
            }],
            commands: IndexMap::from([("start".to_owned(), "node server.js".to_owned())]),
            cwd: Some("/app".to_owned()),
            prepare: None,
            mounts: Some(vec![Mount {
                name: "app".to_owned(),
                build_path: PathBuf::from("unused"),
                serve_path: PathBuf::from("/app"),
            }]),
            volumes: None,
            env: Some(IndexMap::from([(
                "NODE_ENV".to_owned(),
                "production".to_owned(),
            )])),
            services: None,
        }
    }

    #[test]
    fn dockerfile_packages_artifacts_into_a_runtime_image() {
        let temporary = tempfile::tempdir().unwrap();
        let runner = runner(temporary.path());
        let serve = serve();
        runner.write_scripts(&serve).unwrap();

        let dockerfile = runner.dockerfile_contents(&serve).unwrap();

        assert!(dockerfile.contains("FROM debian:trixie-slim AS runtime-tools"));
        assert!(dockerfile.contains(
            "RUN --mount=type=cache,target=/mise/cache,sharing=locked mise use --global \"node@22\""
        ));
        assert!(dockerfile.contains("FROM debian:trixie-slim AS runtime"));
        assert!(dockerfile.contains(&format!("COPY --from={LAMBDA_ADAPTER_IMAGE}")));
        assert!(!dockerfile.contains("ghcr.io"));
        assert!(dockerfile.contains("COPY [\".anybuild/local/build/app\",\"/app\"]"));
        assert!(dockerfile.contains("ENV NODE_ENV=\"production\""));
        assert!(dockerfile.contains("ENV HOST=\"0.0.0.0\""));
        assert!(dockerfile.contains("ENV AWS_LWA_PORT=8080"));
        assert!(!dockerfile.contains("build-essential"));
        assert!(!dockerfile.contains("COPY --from=runtime-tools /usr/lib /usr/lib"));
        assert!(dockerfile.contains("WORKDIR \"/app\""));
        assert!(dockerfile.contains("ENTRYPOINT [\"/anybuild/bin/entrypoint\"]"));
        assert!(dockerfile.contains("EXPOSE 8080"));
        assert!(dockerfile.contains("CMD [\"start\"]"));
        assert_eq!(
            DockerRunner::image_name(&serve.name),
            crate::build::docker::internal_image_name("Acme Web")
        );
    }

    #[test]
    fn dockerfile_installs_sendmail_in_the_final_runtime_without_mise() {
        for with_node in [false, true] {
            let temporary = tempfile::tempdir().unwrap();
            let runner = runner(temporary.path());
            let mut serve = serve();
            if !with_node {
                serve.deps.clear();
            }
            serve.deps.push(Package {
                name: "sendmail".to_owned(),
                version: None,
                architecture: None,
            });
            let dockerfile = runner.dockerfile_contents(&serve).unwrap();
            let runtime = dockerfile
                .find("FROM debian:trixie-slim AS runtime\n")
                .unwrap();
            let install = dockerfile.find("install sendmail-bin").unwrap();
            assert!(install > runtime);
            assert!(!dockerfile.contains("mise use --global \"sendmail"));
            assert_eq!(dockerfile.contains("RUN curl https://mise.run"), with_node);
        }
    }

    #[test]
    fn dockerfile_uses_the_adapter_image_from_the_operation_environment() {
        let temporary = tempfile::tempdir().unwrap();
        let image = concat!(
            "ghcr.io/wasmerio/aws-lambda-adapter:1.0.0@",
            "sha256:b4da35991627bdac98a81c377d0cc28e6989687359576dfda9f0b64be835d648"
        );
        let runner = runner_with_env(
            temporary.path(),
            IndexMap::from([("ANYBUILD_LAMBDA_ADAPTER_IMAGE".to_owned(), image.to_owned())]),
        );

        let dockerfile = runner.dockerfile_contents(&serve()).unwrap();
        assert!(dockerfile.contains(&format!(
            "COPY --from={image} /lambda-adapter /opt/extensions/lambda-adapter"
        )));
        assert!(!dockerfile.contains("public.ecr.aws"));
    }

    #[test]
    fn dockerfile_uses_the_default_adapter_for_an_empty_override() {
        let temporary = tempfile::tempdir().unwrap();
        let runner = runner_with_env(
            temporary.path(),
            IndexMap::from([("ANYBUILD_LAMBDA_ADAPTER_IMAGE".to_owned(), String::new())]),
        );

        let dockerfile = runner.dockerfile_contents(&serve()).unwrap();
        assert!(dockerfile.contains(&format!("COPY --from={LAMBDA_ADAPTER_IMAGE}")));
    }

    #[test]
    fn dockerfile_rejects_adapter_overrides_with_whitespace() {
        for image in [
            "mirror/adapter:1.0.0 extra",
            "mirror/adapter:1.0.0\nRUN false",
        ] {
            let temporary = tempfile::tempdir().unwrap();
            let runner = runner_with_env(
                temporary.path(),
                IndexMap::from([("ANYBUILD_LAMBDA_ADAPTER_IMAGE".to_owned(), image.to_owned())]),
            );

            let error = runner.dockerfile_contents(&serve()).unwrap_err();
            assert!(error.to_string().contains("ANYBUILD_LAMBDA_ADAPTER_IMAGE"));
        }
    }

    #[test]
    fn dockerfile_adds_python_virtualenv_to_path() {
        let temporary = tempfile::tempdir().unwrap();
        let runner = runner(temporary.path());
        let mut serve = serve();
        serve.mounts = Some(vec![Mount {
            name: "venv".to_owned(),
            build_path: PathBuf::from("unused"),
            serve_path: PathBuf::from("/opt/venv"),
        }]);

        let dockerfile = runner.dockerfile_contents(&serve).unwrap();
        assert!(dockerfile.contains("ENV PATH=\"/opt/venv/bin:$PATH\""));
    }

    #[test]
    fn dockerfile_preserves_explicit_host() {
        let temporary = tempfile::tempdir().unwrap();
        let runner = runner(temporary.path());
        let mut serve = serve();
        serve
            .env
            .as_mut()
            .unwrap()
            .insert("HOST".to_owned(), "127.0.0.1".to_owned());

        let dockerfile = runner.dockerfile_contents(&serve).unwrap();
        assert!(dockerfile.contains("ENV HOST=\"127.0.0.1\""));
        assert!(!dockerfile.contains("ENV HOST=\"0.0.0.0\""));
    }

    #[test]
    fn loopback_database_hosts_use_the_docker_host_gateway() {
        assert_eq!(
            docker_container_env_value("DB_HOST", "127.0.0.1"),
            "host.docker.internal"
        );
        assert_eq!(
            docker_container_env_value("DATABASE_HOST", "localhost"),
            "host.docker.internal"
        );
        assert_eq!(
            docker_container_env_value("DB_HOST", "database.internal"),
            "database.internal"
        );
        assert_eq!(docker_container_env_value("HOST", "localhost"), "localhost");
    }

    #[test]
    fn dockerignore_only_sends_runtime_inputs() {
        let temporary = tempfile::tempdir().unwrap();
        let runner = runner(temporary.path());
        let serve = serve();
        runner.write_scripts(&serve).unwrap();
        runner.write_dockerignore(&serve).unwrap();

        let dockerignore = std::fs::read_to_string(&runner.dockerignore_path).unwrap();
        assert!(dockerignore.starts_with("**\n"));
        assert!(dockerignore.contains("!.anybuild/local/build/app/**"));
        assert!(dockerignore.contains("!.anybuild/runner/docker/bin/**"));
    }
}
