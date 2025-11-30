use std::collections::BTreeMap;
use std::fs;
use std::process::Command;

use base64::Engine;
use camino::Utf8PathBuf;

use crate::Result;
use crate::assets;
use crate::builder::Builder;
use crate::model::{Mount, PrepareStep, Serve, Step};

/// Docker builder emits Dockerfiles and builds images mirroring Python behavior.
///
/// This implementation is intentionally minimal: it materializes build steps
/// into a Docker context under `.shipit/docker/context`, writes a simple
/// Dockerfile, builds an image named `shipit-local:latest` using the
/// configured docker client, and exposes a `run_serve_command` helper that
/// invokes `docker run` on that image.
///
/// The goal is to provide working Docker backend support for typical
/// simple/static flows. Edge-cases and the full Python behavior (multi-stage
/// mise installs, complex dependency handling) can be iterated on later.
pub struct DockerBuilder {
    pub src_dir: Utf8PathBuf,
    pub docker_client: String,
    /// Accumulates Dockerfile fragments while building so `finalize_build`
    /// can emit a complete multi-stage Dockerfile.
    docker_file_contents: String,
    /// Image name tagged during finalize_build for reuse when running.
    image_name: Option<String>,
    /// Dependency mapping for mise, like Python's mise_mapper.
    mise_mapper: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
    /// Store the serve for volume mappings in run_serve_command.
    serve: Option<Serve>,
}

impl DockerBuilder {
    pub fn new(src_dir: Utf8PathBuf, docker_client: Option<String>) -> Self {
        let mut mise_mapper = std::collections::BTreeMap::new();
        mise_mapper.insert(
            "php".to_string(),
            [("source".to_string(), "ubi:adwinying/php".to_string())].into(),
        );
        mise_mapper.insert(
            "composer".to_string(),
            [
                ("source".to_string(), "ubi:composer/composer".to_string()),
                ("postinstall".to_string(), "composer_dir=$(mise where ubi:composer/composer); ln -s \"$composer_dir/composer.phar\" /usr/local/bin/composer".to_string()),
            ].into(),
        );
        Self {
            src_dir,
            docker_client: docker_client.unwrap_or_else(|| "docker".to_string()),
            docker_file_contents: String::new(),
            image_name: None,
            mise_mapper,
            serve: None,
        }
    }

    /// `.shipit/docker` directory inside the project.
    fn docker_dir(&self) -> Utf8PathBuf {
        self.src_dir.join(".shipit/docker")
    }

    // `.shipit/docker/out` output path for docker build --output (kept for
    // parity/debugging even if unused in the minimal flow).
    #[allow(dead_code)]
    fn docker_out_path(&self) -> Utf8PathBuf {
        self.docker_dir().join("out")
    }

    /// Prefix used in the Dockerfile for internal shipit paths.
    fn shipit_docker_prefix(&self) -> String {
        "/shipit".to_string()
    }

    /// Emit a `RUN mkdir -p` into the Dockerfile and return the absolute path
    /// used inside the Dockerfile.
    fn mkdir(&mut self, path: &Utf8PathBuf) -> Utf8PathBuf {
        // Ensure we build a normalized path fragment for embedding under /shipit.
        // Utf8PathBuf does not implement trim_start_matches directly, so convert
        // to &str first and trim leading slashes.
        let trimmed = path.as_str().trim_start_matches('/').to_string();
        let full = format!("{}/{}", self.shipit_docker_prefix(), trimmed);
        self.docker_file_contents
            .push_str(&format!("RUN mkdir -p {}\n", full));
        Utf8PathBuf::from(full)
    }

    /// Emit a heredoc file creation into the Dockerfile and a chmod for it.
    fn create_file(&mut self, path: &Utf8PathBuf, content: &str, mode: u32) -> Utf8PathBuf {
        let full = if path.is_absolute() {
            path.to_string()
        } else {
            // Convert to &str and trim leading '/' characters before composing.
            let trimmed = path.as_str().trim_start_matches('/').to_string();
            format!("{}/{}", self.shipit_docker_prefix(), trimmed)
        };
        self.docker_file_contents.push_str("\nRUN cat > ");
        self.docker_file_contents.push_str(&full);
        self.docker_file_contents.push_str(" <<'EOF'\n");
        self.docker_file_contents.push_str(content);
        if !content.ends_with('\n') {
            self.docker_file_contents.push_str("\n");
        }
        self.docker_file_contents.push_str("EOF\n\n");
        self.docker_file_contents
            .push_str(&format!("RUN chmod {:o} {}\n\n", mode, full));
        Utf8PathBuf::from(full)
    }

    /// Add dependency install instructions into the Dockerfile. Mirrors Python
    /// `mise_mapper` behavior for common tools and falls back to `mise use`.
    fn add_dependency(&mut self, name: &str, version: &Option<String>) {
        match name {
            "pie" => {
                self.docker_file_contents.push_str("RUN apt-get update && apt-get -y --no-install-recommends install gcc make autoconf libtool bison re2c pkg-config libpq-dev\n");
                self.docker_file_contents.push_str("RUN curl -L --output /usr/bin/pie https://github.com/php/pie/releases/download/1.2.0/pie.phar && chmod +x /usr/bin/pie\n");
            }
            "static-web-server" => {
                if let Some(v) = version {
                    self.docker_file_contents
                        .push_str(&format!("ENV SWS_INSTALL_VERSION={}\n", v));
                }
                self.docker_file_contents.push_str("RUN curl --proto '=https' --tlsv1.2 -sSfL https://get.static-web-server.net | sh\n");
            }
            other => {
                let mapped = self.mise_mapper.get(other);
                let package_name = mapped
                    .and_then(|m| m.get("source"))
                    .map(|s| s.as_str())
                    .unwrap_or(other);
                if let Some(v) = version {
                    self.docker_file_contents
                        .push_str(&format!("RUN mise use --global {}@{}\n", package_name, v));
                } else {
                    self.docker_file_contents
                        .push_str(&format!("RUN mise use --global {}\n", package_name));
                }
                if let Some(post) = mapped.and_then(|m| m.get("postinstall")) {
                    self.docker_file_contents
                        .push_str(&format!("RUN {}\n", post));
                }
            }
        }
    }
}

impl Builder for DockerBuilder {
    fn build(
        &mut self,
        _env: &BTreeMap<String, String>,
        _mounts: &[Mount],
        steps: &[Step],
    ) -> Result<()> {
        // Set up the Dockerfile build stage
        self.docker_file_contents = "# syntax=docker/dockerfile:1.7-labs\n".to_string();
        self.docker_file_contents
            .push_str("FROM debian:trixie-slim AS build\n\n");
        self.docker_file_contents.push_str("RUN apt-get update \\\n    && apt-get -y --no-install-recommends install \\\n        build-essential gcc make autoconf libtool bison \\\n        dpkg-dev pkg-config re2c locate \\\n        libmariadb-dev libmariadb-dev-compat libpq-dev \\\n        libvips-dev default-libmysqlclient-dev libmagickwand-dev \\\n        libicu-dev libxml2-dev libxslt-dev libyaml-dev \\\n        sudo curl ca-certificates \\\n    && rm -rf /var/lib/apt/lists/*\n\n");
        self.docker_file_contents
            .push_str("SHELL [\"/bin/bash\", \"-o\", \"pipefail\", \"-c\"]\n");
        self.docker_file_contents.push_str("ENV MISE_DATA_DIR=\"/mise\"\nENV MISE_CONFIG_DIR=\"/mise\"\nENV MISE_CACHE_DIR=\"/mise/cache\"\nENV MISE_INSTALL_PATH=\"/usr/local/bin/mise\"\nENV PATH=\"/mise/shims:$PATH\"\n\n");
        self.docker_file_contents
            .push_str("RUN curl https://mise.run | sh\n\n");

        // Ensure mount directories exist in the build stage.
        for mount in _mounts {
            self.docker_file_contents
                .push_str(&format!("RUN mkdir -p {}\n", mount.build_path));
        }
        self.docker_file_contents.push_str("\n");

        // Render build steps.
        for step in steps {
            match step {
                Step::Workdir(w) => {
                    self.docker_file_contents
                        .push_str(&format!("WORKDIR {}\n", w.path));
                }
                Step::Run(r) => {
                    if !r.inputs.is_empty() {
                        let mounts = r
                            .inputs
                            .iter()
                            .map(|input| {
                                format!(
                                    "--mount=type=bind,source={},target={} \\\n  ",
                                    input, input
                                )
                            })
                            .collect::<String>();
                        self.docker_file_contents
                            .push_str(&format!("RUN {}{}\n", mounts, r.command));
                    } else {
                        self.docker_file_contents
                            .push_str(&format!("RUN {}\n", r.command));
                    }
                }
                Step::Copy(c) => {
                    if c.is_download() {
                        self.docker_file_contents
                            .push_str(&format!("ADD {} {}\n", c.source, c.target));
                    } else if matches!(c.base, crate::model::CopyBase::Assets) {
                        if let Some(data) = assets::get_asset(&c.source) {
                            let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
                            self.docker_file_contents.push_str(&format!(
                                "RUN echo '{b64}' | base64 -d > {target}\n",
                                b64 = b64,
                                target = c.target
                            ));
                        } else {
                            return Err(anyhow::anyhow!("Asset {} not found", c.source));
                        }
                    } else {
                        if !c.ignore.is_empty() {
                            let mut exclude = String::from(" \\\n  ");
                            for ig in &c.ignore {
                                exclude.push_str(&format!("  --exclude={}\\\n", ig));
                            }
                            exclude.push_str(" \\\n ");
                            self.docker_file_contents
                                .push_str(&format!("COPY{} {} {}\n", exclude, c.source, c.target));
                        } else {
                            self.docker_file_contents
                                .push_str(&format!("COPY {} {}\n", c.source, c.target));
                        }
                    }
                }
                Step::Env(e) => {
                    for (k, v) in &e.variables {
                        self.docker_file_contents
                            .push_str(&format!("ENV {}={}\n", k, v));
                    }
                }
                Step::Path(p) => {
                    self.docker_file_contents
                        .push_str(&format!("ENV PATH={}:$PATH\n", p.path));
                }
                Step::Use(u) => {
                    for dependency in &u.dependencies {
                        self.add_dependency(&dependency.name, &dependency.version);
                    }
                }
            }
        }

        Ok(())
    }

    fn build_prepare(&mut self, _serve: &Serve) -> Result<()> {
        Ok(())
    }

    fn prepare(&mut self, _env: &BTreeMap<String, String>, _prepare: &[PrepareStep]) -> Result<()> {
        // No-op for simple Docker flow; prepare steps can be executed in-built
        // in the Dockerfile later if needed.
        Ok(())
    }

    fn build_serve(&mut self, serve: &Serve) -> Result<()> {
        // Emit serve scripts into the Dockerfile using helper heredoc creation.
        // This mirrors Python's `create_file` behavior which writes files under
        // `/shipit/serve/bin` inside the image.
        let serve_bin = Utf8PathBuf::from("serve/bin");
        let _ = self.mkdir(&serve_bin);

        for (name, cmd) in &serve.commands {
            let path = Utf8PathBuf::from(format!("serve/bin/{}", name));
            let content = if let Some(cwd) = &serve.cwd {
                format!("#!/usr/bin/env bash\nset -euo pipefail\ncd {cwd}\n{cmd}\n")
            } else {
                format!("#!/usr/bin/env bash\nset -euo pipefail\ncd /app\n{cmd}\n")
            };
            self.create_file(&path, &content, 0o755);
        }

        // Ensure serve-level dependencies are present in the build image.
        for dep in &serve.deps {
            self.add_dependency(&dep.name, &dep.version);
        }

        Ok(())
    }

    fn finalize_build(&mut self, serve: &Serve) -> Result<()> {
        self.serve = Some(serve.clone());

        let docker_dir = self.src_dir.join(".shipit/docker");
        if docker_dir.exists() {
            fs::remove_dir_all(&docker_dir)?;
        }
        fs::create_dir_all(&docker_dir)?;

        // Final stage: copy from build to scratch
        self.docker_file_contents.push_str("\nFROM scratch\n");
        if let Some(mounts) = &serve.mounts {
            for mount in mounts {
                self.docker_file_contents.push_str(&format!(
                    "COPY --from=build {} {}\n",
                    mount.build_path, mount.build_path
                ));
            }
        } else {
            self.docker_file_contents
                .push_str("COPY --from=build /app /app\n");
        }

        // Write Dockerfile
        let dockerfile_path = docker_dir.join("Dockerfile");
        fs::write(&dockerfile_path, &self.docker_file_contents)?;

        let dockerignore_path = docker_dir.join("Dockerfile.dockerignore");
        fs::write(&dockerignore_path, ".shipit\nShipit\n")?;

        // Build the docker image
        let out_dir = self.src_dir.join(".shipit/docker/out");
        if out_dir.exists() {
            fs::remove_dir_all(&out_dir)?;
        }
        fs::create_dir_all(&out_dir)?;

        let docker_bin = self.docker_client.clone();
        let mut cmd = Command::new(&docker_bin);
        cmd.arg("build");
        cmd.arg("-f");
        cmd.arg(dockerfile_path.to_string());
        cmd.arg("-t");
        cmd.arg(&serve.name);
        let stable_image = "shipit-local:latest".to_string();
        cmd.arg("-t");
        cmd.arg(&stable_image);
        self.image_name = Some(serve.name.clone());
        cmd.arg("--platform");
        cmd.arg("linux/amd64");
        cmd.arg("--output");
        cmd.arg(out_dir.to_string());
        cmd.arg(".");
        cmd.current_dir(&self.src_dir);
        cmd.envs(std::env::vars());
        cmd.env("DOCKER_BUILDKIT", "1");
        if self.docker_client == "depot" {
            let metadata = self.docker_dir().join("depot-build.json");
            cmd.arg("--save");
            cmd.arg(format!("--metadata-file={}", metadata.to_string()));
        }
        let status = cmd.status()?;
        if !status.success() {
            return Err(anyhow::anyhow!("docker build failed with {}", status));
        }

        Ok(())
    }

    fn getenv(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn run_serve_command(&mut self, _command: &str) -> Result<()> {
        // Run the previously built image and proxy the configured port.
        // We map HOST:CONTAINER port to 80, matching Python implementation.
        let port = "80".to_string();
        let image = self
            .image_name
            .clone()
            .unwrap_or_else(|| "shipit-local:latest".to_string());
        let mut args = vec![
            "run".to_string(),
            "--rm".to_string(),
            "-p".to_string(),
            format!("{}:{}", port, port),
        ];
        if let Some(serve) = &self.serve {
            if let Some(volumes) = &serve.volumes {
                for vol in volumes {
                    args.push("--mount".to_string());
                    args.push(format!(
                        "type=volume,source={},target={}",
                        vol.name, vol.serve_path
                    ));
                }
            }
        }
        args.push(image);
        let docker_bin = self.docker_client.clone();
        self.run_command(&docker_bin, Some(&args))?;
        Ok(())
    }

    fn run_command(&mut self, command: &str, extra_args: Option<&[String]>) -> Result<()> {
        tracing::info!(command, ?extra_args, "invoking docker client");
        let mut cmd = Command::new(command);
        if let Some(args) = extra_args {
            cmd.args(args);
        }
        // Execute commands from the docker directory when applicable.
        let workdir = self.src_dir.join(".shipit/docker");
        cmd.current_dir(&workdir);
        let status = cmd.status()?;
        if !status.success() {
            return Err(anyhow::anyhow!(
                "Command {} failed with {}",
                command,
                status
            ));
        }
        Ok(())
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

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
