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

const TOOLCHAIN_STAGE: &str = r#"# syntax=docker/dockerfile:1.7-labs
FROM debian:trixie-slim AS runtime-tools

RUN apt-get update \
    && apt-get -y --no-install-recommends install \
        curl ca-certificates unzip git xz-utils \
    && rm -rf /var/lib/apt/lists/*

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

const NATIVE_BUILD_DEPS: &str = r#"RUN apt-get update \
    && apt-get -y --no-install-recommends install \
        build-essential gcc make autoconf libtool bison \
        dpkg-dev pkg-config re2c locate \
        libmariadb-dev libmariadb-dev-compat libpq-dev libsqlite3-dev \
        libvips-dev default-libmysqlclient-dev libmagickwand-dev \
        libicu-dev libxml2-dev libxslt1-dev libyaml-dev \
    && rm -rf /var/lib/apt/lists/*
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
        let normalized: String = serve_name
            .chars()
            .map(|character| {
                let character = character.to_ascii_lowercase();
                if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                    character
                } else {
                    '-'
                }
            })
            .collect();
        let normalized = normalized.trim_matches(['.', '_', '-']);
        if normalized.is_empty() {
            "anybuild-app".to_owned()
        } else {
            normalized.to_owned()
        }
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
        let mut contents = String::from(TOOLCHAIN_STAGE);
        let uses_mise = serve.deps.iter().any(|dependency| {
            !matches!(
                dependency.name.as_str(),
                "bash" | "composer" | "pie" | "static-web-server"
            )
        });
        let builds_native_runtime = serve
            .deps
            .iter()
            .any(|dependency| matches!(dependency.name.as_str(), "php" | "python" | "pie"));
        if builds_native_runtime {
            contents.push_str(NATIVE_BUILD_DEPS);
        }
        if uses_mise {
            contents.push_str(MISE_SETUP);
        }
        for dependency in &serve.deps {
            contents.push_str(&dependency_install_contents(dependency));
        }
        if uses_mise {
            contents.push_str("RUN rm -rf /mise/cache\n");
        }
        contents.push_str(RUNTIME_STAGE);
        if uses_mise {
            contents.push_str(
                "# Copy resolved toolchains and shared libraries without compilers or caches.\n",
            );
            contents.push_str("COPY --from=runtime-tools /mise /mise\n");
            contents.push_str("COPY --from=runtime-tools /usr/local/bin /usr/local/bin\n");
            contents.push_str("COPY --from=runtime-tools /usr/lib /usr/lib\n");
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
    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn prepare_build_steps(&self, steps: Vec<Step>) -> Vec<Step> {
        steps
    }

    fn build(&mut self, serve: &Serve) -> Result<()> {
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
        Ok(())
    }

    fn prepare(&mut self, _env: &IndexMap<String, String>, prepare: &[RunStep]) -> Result<()> {
        if prepare.is_empty() {
            return Ok(());
        }
        let mappings = load_volume_mappings(&self.src_dir, Some(&self.anybuild_dir))?;
        self.run_serve_command("prepare", Some(&mappings), &[], None)
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
    use crate::plan::{Mount, Package};

    fn runner(root: &Path) -> DockerRunner {
        let source = root.join("src");
        let anybuild_dir = source.join(".anybuild");
        std::fs::create_dir_all(&source).unwrap();
        let backend: Rc<RefCell<dyn BuildBackend>> = Rc::new(RefCell::new(LocalBuildBackend::new(
            source.clone(),
            root.join("assets"),
            Some(anybuild_dir.clone()),
            OperationContext::for_test(),
        )));
        std::fs::create_dir_all(backend.borrow().get_artifact_mount_path("app")).unwrap();
        DockerRunner::new(
            backend,
            source,
            None,
            None,
            Some(anybuild_dir),
            OperationContext::for_test(),
        )
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
        assert!(dockerfile.contains("RUN mise use --global \"node@22\""));
        assert!(dockerfile.contains("FROM debian:trixie-slim AS runtime"));
        assert!(dockerfile.contains("COPY [\".anybuild/local/build/app\",\"/app\"]"));
        assert!(dockerfile.contains("ENV NODE_ENV=\"production\""));
        assert!(dockerfile.contains("WORKDIR \"/app\""));
        assert!(dockerfile.contains("ENTRYPOINT [\"/anybuild/bin/entrypoint\"]"));
        assert!(dockerfile.contains("EXPOSE 8080"));
        assert!(dockerfile.contains("CMD [\"start\"]"));
        assert_eq!(DockerRunner::image_name(&serve.name), "acme-web");
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
