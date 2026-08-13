//! Docker build backend.
//!
//! Same synthesized-execution approach as Python: every plan step is
//! rendered into a single containerized Dockerfile (build stage on
//! debian:trixie-slim with mise, artifacts exported from a scratch
//! stage via `--output`), then the selected docker client (docker /
//! depot / podman via CLI arg) runs the build as a subprocess.

use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use base64::Engine as _;
use indexmap::IndexMap;

use crate::operation::OperationContext;
use crate::plan::{Mount, Package, Step};

use crate::build::BuildBackend;

/// Port of `DockerBuildBackend.mise_mapper`.
fn mise_source(dependency: &str) -> Option<&'static str> {
    Some(match dependency {
        "php" => "ubi:adwinying/php",
        "go-wasix" => "ubi:wasix-org/go[extract_all=true,bin_path=bin/]",
        "composer" => "ubi:composer/composer",
        "hugo" => "ubi:gohugoio/hugo[matching=extended]",
        _ => return None,
    })
}

fn mise_postinstall(dependency: &str) -> Option<&'static str> {
    match dependency {
        "composer" => Some(
            r#"composer_dir=$(mise where ubi:composer/composer); ln -s "$composer_dir/composer.phar" /usr/local/bin/composer"#,
        ),
        _ => None,
    }
}

pub(crate) fn dependency_install_contents(dependency: &Package) -> String {
    let mut contents = String::new();
    if dependency.name == "bash" {
        return contents;
    }
    if dependency.name == "pie" {
        contents.push_str(
            "RUN apt-get update && apt-get -y --no-install-recommends install gcc make autoconf libtool bison re2c pkg-config libpq-dev\n",
        );
        contents.push_str(
            "RUN curl -L --output /usr/bin/pie https://github.com/php/pie/releases/download/1.2.0/pie.phar && chmod +x /usr/bin/pie\n",
        );
        return contents;
    }
    if dependency.name == "composer" {
        let version = dependency.version.as_deref().unwrap_or("2.9.2");
        contents.push_str(&format!(
            "RUN curl -L --output /usr/bin/composer https://github.com/composer/composer/releases/download/{version}/composer.phar && chmod +x /usr/bin/composer\n"
        ));
        return contents;
    }
    if dependency.name == "static-web-server" {
        if let Some(version) = dependency.version.as_deref() {
            contents.push_str(&format!("ENV SWS_INSTALL_VERSION={version}\n"));
        }
        contents.push_str(
            "RUN curl --proto '=https' --tlsv1.2 -sSfL https://get.static-web-server.net | sh\n",
        );
        return contents;
    }

    let package_name = mise_source(&dependency.name).unwrap_or(dependency.name.as_str());
    if let Some(version) = dependency.version.as_deref() {
        contents.push_str(&format!(
            "RUN mise use --global \"{package_name}@{version}\"\n"
        ));
    } else {
        contents.push_str(&format!("RUN mise use --global \"{package_name}\"\n"));
    }
    if let Some(postinstall) = mise_postinstall(&dependency.name) {
        contents.push_str(&format!("RUN {postinstall}\n"));
    }
    contents
}

/// The build-stage preamble (verbatim from docker.py).
const DOCKERFILE_HEADER: &str = "\
# syntax=docker/dockerfile:1.7-labs
FROM debian:trixie-slim AS build

RUN apt-get update \\
    && apt-get -y --no-install-recommends install \\
        build-essential gcc make autoconf libtool bison \\
        dpkg-dev pkg-config re2c locate \\
        libmariadb-dev libmariadb-dev-compat libpq-dev libsqlite3-dev \\
        libvips-dev default-libmysqlclient-dev libmagickwand-dev \\
        libicu-dev libxml2-dev libxslt-dev libyaml-dev \\
        sudo curl ca-certificates unzip git \\
    && rm -rf /var/lib/apt/lists/*

SHELL [\"/bin/bash\", \"-o\", \"pipefail\", \"-c\"]
ENV MISE_DATA_DIR=\"/mise\"
ENV MISE_CONFIG_DIR=\"/mise\"
ENV MISE_CACHE_DIR=\"/mise/cache\"
ENV MISE_INSTALL_PATH=\"/usr/local/bin/mise\"
ENV PATH=\"/mise/shims:$PATH\"

RUN curl https://mise.run | sh
";

/// Python's `Path.absolute()`: prefix with the cwd, no normalization.
fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

fn path_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Port of `builders/docker.py::DockerBuildBackend`.
pub struct DockerBuildBackend {
    pub src_dir: PathBuf,
    pub assets_path: PathBuf,
    pub anybuild_dir: PathBuf,
    pub docker_path: PathBuf,
    pub docker_out_path: PathBuf,
    pub docker_file_path: PathBuf,
    pub docker_name_path: PathBuf,
    pub docker_ignore_path: PathBuf,
    pub docker_client: String,
    pub docker_opts: Option<String>,
    runtime_path: Option<String>,
    operation: OperationContext,
}

impl DockerBuildBackend {
    pub fn new(
        src_dir: PathBuf,
        assets_path: PathBuf,
        docker_client: Option<String>,
        docker_opts: Option<String>,
        anybuild_dir: Option<PathBuf>,
        operation: OperationContext,
    ) -> Result<Self> {
        let anybuild_dir = anybuild_dir.unwrap_or_else(|| src_dir.join(".anybuild"));
        let docker_path = anybuild_dir.join("docker");
        std::fs::create_dir_all(&docker_path)
            .with_context(|| format!("creating {}", docker_path.display()))?;
        Ok(Self {
            src_dir,
            assets_path,
            anybuild_dir,
            docker_out_path: docker_path.join("out"),
            docker_file_path: docker_path.join("Dockerfile"),
            docker_name_path: docker_path.join("name"),
            docker_ignore_path: docker_path.join("Dockerfile.dockerignore"),
            docker_path,
            docker_client: docker_client.unwrap_or_else(|| "docker".to_owned()),
            docker_opts,
            runtime_path: None,
            operation,
        })
    }

    /// Port of `get_mount_path` (relative form).
    fn get_mount_path(&self, name: &str) -> PathBuf {
        if name == "app" {
            PathBuf::from("app")
        } else {
            PathBuf::from("opt").join(name)
        }
    }

    /// Port of `print_dockerfile` (rich Panel(Syntax(..., "dockerfile"))).
    fn print_dockerfile(&self, contents: &str) {
        crate::build::report::print_syntax_panel(&self.operation, contents, "dockerfile");
    }

    /// Port of `build_dockerfile`: write the Dockerfile and run the
    /// selected docker client.
    fn build_dockerfile(&self, image_name: &str, contents: &str) -> Result<()> {
        std::fs::write(&self.docker_file_path, contents)?;
        std::fs::write(&self.docker_name_path, image_name)?;
        self.print_dockerfile(contents);
        let mut extra_args: Vec<String> = Vec::new();
        if let Some(docker_opts) = &self.docker_opts {
            // Python appends the raw opts string as a single argument.
            extra_args.push(docker_opts.clone());
        }
        let mut cmd = std::process::Command::new(&self.docker_client);
        self.operation.prepare_command(&mut cmd);
        cmd.arg("build")
            .arg("-f")
            .arg(absolute_path(&self.docker_file_path))
            .arg("-t")
            .arg(image_name)
            .arg("--platform")
            .arg("linux/amd64")
            .arg("--output")
            .arg(absolute_path(&self.docker_out_path))
            .arg(".")
            .args(&extra_args)
            .current_dir(absolute_path(&self.src_dir));
        let status = self
            .operation
            .command_status(&mut cmd)
            .with_context(|| format!("failed to run {}", self.docker_client))?;
        ensure!(
            status.success(),
            "Command {} build failed with exit code {:?}",
            self.docker_client,
            status.code()
        );
        Ok(())
    }

    /// The Dockerfile synthesis from `build` (factored out so it is unit
    /// testable without a docker client). Mutates `env` exactly as the
    /// Python step loop does; the final PATH becomes the runtime path.
    fn dockerfile_contents(
        &self,
        env: &mut IndexMap<String, String>,
        mounts: &[Mount],
        steps: &[Step],
    ) -> Result<String> {
        let mut docker_file_contents = String::from(DOCKERFILE_HEADER);

        for mount in mounts {
            docker_file_contents.push_str(&format!(
                "RUN mkdir -p {}\n",
                path_str(&absolute_path(&mount.build_path))
            ));
        }

        for step in steps {
            match step {
                Step::Workdir(step) => {
                    docker_file_contents.push_str(&format!(
                        "WORKDIR {}\n",
                        path_str(&absolute_path(&step.path))
                    ));
                }
                Step::Run(step) => {
                    let inputs = step.inputs.as_deref().unwrap_or(&[]);
                    let pre = if !inputs.is_empty() {
                        let mut parents: Vec<String> = inputs
                            .iter()
                            .filter_map(|input| {
                                let parent = Path::new(input).parent()?;
                                let parent = path_str(parent);
                                (!parent.is_empty() && parent != ".").then_some(parent)
                            })
                            .collect();
                        parents.sort();
                        parents.dedup();
                        for parent in &parents {
                            docker_file_contents.push_str(&format!("RUN mkdir -p {parent}\n"));
                        }
                        let mut pre = String::from("\\\n  ");
                        for input in inputs {
                            pre.push_str(&format!(
                                "--mount=type=bind,source={input},target={input} \\\n  "
                            ));
                        }
                        pre
                    } else {
                        String::new()
                    };
                    docker_file_contents.push_str(&format!("RUN {pre}{}\n", step.command));
                }
                Step::Copy(step) => {
                    if step.is_download() {
                        docker_file_contents
                            .push_str(&format!("ADD {} {}\n", step.source, step.target));
                    } else if step.base == "assets" {
                        let asset_path = self.assets_path.join(&step.source);
                        if asset_path.is_file() {
                            let content_base64 = b64(&std::fs::read(&asset_path)?);
                            docker_file_contents.push_str(&format!(
                                "RUN echo '{content_base64}' | base64 -d > {}\n",
                                step.target
                            ));
                        } else if asset_path.is_dir() {
                            bail!(
                                "Asset {} is a directory, anybuild doesn't currently support coppying assets directories inside Docker",
                                step.source
                            );
                        } else {
                            bail!("Asset {} does not exist", step.source);
                        }
                    } else {
                        let exclude = match step.ignore.as_deref() {
                            Some(ignore) if !ignore.is_empty() => {
                                let items: Vec<String> = ignore
                                    .iter()
                                    .map(|ignore| format!("  --exclude={ignore}"))
                                    .collect();
                                format!(" \\\n{} \\\n ", items.join(" \\\n"))
                            }
                            _ => String::new(),
                        };
                        docker_file_contents
                            .push_str(&format!("COPY{exclude} {} {}\n", step.source, step.target));
                    }
                }
                Step::Env(step) => {
                    let env_vars: Vec<String> = step
                        .variables
                        .iter()
                        .map(|(key, value)| format!("{key}={value}"))
                        .collect();
                    docker_file_contents.push_str(&format!("ENV {}\n", env_vars.join(" ")));
                    for (key, value) in &step.variables {
                        env.insert(key.clone(), value.clone());
                    }
                }
                Step::Path(step) => {
                    docker_file_contents.push_str(&format!("ENV PATH={}:$PATH\n", step.path));
                    let pathsep = if cfg!(windows) { ';' } else { ':' };
                    let current = env.get("PATH").cloned().unwrap_or_default();
                    env.insert(
                        "PATH".to_owned(),
                        format!("{}{pathsep}{current}", step.path),
                    );
                }
                Step::WriteFile(step) => {
                    let content_base64 = b64(step.content.as_bytes());
                    let target_path = Path::new(&step.path);
                    // Python's Path("x").parent is "." (not "").
                    let parent = match target_path.parent().map(path_str) {
                        Some(parent) if !parent.is_empty() => parent,
                        _ => ".".to_owned(),
                    };
                    docker_file_contents.push_str(&format!(
                        "RUN mkdir -p {parent} && echo '{content_base64}' | base64 -d > {}\n",
                        step.path
                    ));
                }
                Step::Use(step) => {
                    for dependency in &step.dependencies {
                        docker_file_contents.push_str(&dependency_install_contents(dependency));
                    }
                }
            }
        }

        docker_file_contents.push_str("\nFROM scratch\n");
        for mount in mounts {
            docker_file_contents.push_str(&format!(
                "COPY --from=build {} {}\n",
                mount.build_path.display(),
                mount.build_path.display()
            ));
        }

        Ok(docker_file_contents)
    }
}

impl BuildBackend for DockerBuildBackend {
    /// Port of `build`: synthesize the Dockerfile from the plan steps and
    /// run the docker client build.
    fn build(
        &mut self,
        name: &str,
        env: &IndexMap<String, String>,
        mounts: &[Mount],
        steps: &[Step],
    ) -> Result<()> {
        let base_path = self.docker_path.clone();
        let _ = std::fs::remove_dir_all(&base_path);
        std::fs::create_dir_all(&base_path)?;

        // The trait passes env immutably; Python mutates the caller's
        // dict. The only observable side channel is the runtime PATH,
        // surfaced through `get_runtime_path`.
        let mut env = env.clone();
        let docker_file_contents = self.dockerfile_contents(&mut env, mounts, steps)?;

        self.runtime_path = env.get("PATH").cloned();

        std::fs::write(
            &self.docker_ignore_path,
            "\n.anybuild\nAnybuild\n.shipit\nShipit\n",
        )?;
        crate::build::report::build_started(&self.operation);
        let started_at = std::time::Instant::now();
        self.build_dockerfile(name, &docker_file_contents)?;
        crate::build::report::success(
            &self.operation,
            format!(
                "Build complete in {:.2}s",
                started_at.elapsed().as_secs_f64()
            ),
        );
        Ok(())
    }

    fn get_build_mount_path(&self, name: &str) -> PathBuf {
        PathBuf::from("/").join(self.get_mount_path(name))
    }

    fn get_artifact_mount_path(&self, name: &str) -> PathBuf {
        self.docker_out_path.join(self.get_mount_path(name))
    }

    fn get_volume_path(&self, name: &str) -> PathBuf {
        self.anybuild_dir.join("volumes").join(name)
    }

    fn get_runtime_path(&self) -> Option<String> {
        self.runtime_path.clone()
    }

    fn artifact_platform(&self) -> Option<&str> {
        Some("linux/amd64")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{
        CopyStep, EnvStep, Package, PathStep, RunStep, UseStep, WorkdirStep, WriteFileStep,
    };

    fn backend(root: &Path) -> DockerBuildBackend {
        DockerBuildBackend::new(
            root.join("src"),
            root.join("assets"),
            None,
            None,
            None,
            OperationContext::for_test(),
        )
        .unwrap()
    }

    #[test]
    fn test_mount_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = backend(tmp.path());
        assert_eq!(backend.get_build_mount_path("app"), PathBuf::from("/app"));
        assert_eq!(
            backend.get_build_mount_path("data"),
            PathBuf::from("/opt/data")
        );
        assert_eq!(
            backend.get_artifact_mount_path("app"),
            backend.docker_out_path.join("app")
        );
        assert_eq!(
            backend.get_artifact_mount_path("data"),
            backend.docker_out_path.join("opt").join("data")
        );
        assert_eq!(
            backend.get_volume_path("db"),
            backend.anybuild_dir.join("volumes").join("db")
        );
    }

    #[test]
    fn test_dockerfile_contents_renders_steps_like_python() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("assets")).unwrap();
        std::fs::write(tmp.path().join("assets").join("cfg.ini"), "hi").unwrap();
        let backend = backend(tmp.path());

        let mounts = vec![Mount {
            name: "app".to_owned(),
            build_path: PathBuf::from("/app"),
            serve_path: PathBuf::from("/app"),
        }];
        let steps = vec![
            Step::Workdir(WorkdirStep {
                path: PathBuf::from("/app"),
            }),
            Step::Use(UseStep {
                dependencies: vec![
                    Package {
                        name: "php".to_owned(),
                        version: Some("8.3".to_owned()),
                        architecture: None,
                    },
                    Package {
                        name: "composer".to_owned(),
                        version: None,
                        architecture: None,
                    },
                    Package {
                        name: "node".to_owned(),
                        version: None,
                        architecture: None,
                    },
                    Package {
                        name: "bash".to_owned(),
                        version: None,
                        architecture: None,
                    },
                ],
            }),
            Step::Env(EnvStep {
                variables: [("FOO".to_owned(), "bar".to_owned())].into_iter().collect(),
            }),
            Step::Path(PathStep {
                path: "/custom/bin".to_owned(),
            }),
            Step::Run(RunStep {
                command: "npm install".to_owned(),
                inputs: Some(vec!["package.json".to_owned(), "web/app.json".to_owned()]),
                outputs: None,
                group: None,
            }),
            Step::Copy(CopyStep {
                source: ".".to_owned(),
                target: "/app".to_owned(),
                ignore: Some(vec!["node_modules".to_owned()]),
                base: "source".to_owned(),
            }),
            Step::Copy(CopyStep {
                source: "cfg.ini".to_owned(),
                target: "/app/cfg.ini".to_owned(),
                ignore: None,
                base: "assets".to_owned(),
            }),
            Step::WriteFile(WriteFileStep {
                path: "/etc/motd".to_owned(),
                content: "hello".to_owned(),
            }),
        ];

        let mut env: IndexMap<String, String> = [("HOME".to_owned(), "/root".to_owned())]
            .into_iter()
            .collect();
        let contents = backend
            .dockerfile_contents(&mut env, &mounts, &steps)
            .unwrap();

        assert!(contents.starts_with(
            "# syntax=docker/dockerfile:1.7-labs\nFROM debian:trixie-slim AS build\n"
        ));
        assert!(contents.contains("RUN curl https://mise.run | sh\n"));
        assert!(contents.contains("RUN mkdir -p /app\n"));
        assert!(contents.contains("WORKDIR /app\n"));
        assert!(contents.contains("RUN mise use --global \"ubi:adwinying/php@8.3\"\n"));
        assert!(contents.contains(
            "RUN curl -L --output /usr/bin/composer https://github.com/composer/composer/releases/download/2.9.2/composer.phar && chmod +x /usr/bin/composer\n"
        ));
        assert!(contents.contains("RUN mise use --global \"node\"\n"));
        assert!(!contents.contains("RUN mise use --global \"bash\"\n"));
        assert!(contents.contains("ENV FOO=bar\n"));
        assert!(contents.contains("ENV PATH=/custom/bin:$PATH\n"));
        assert_eq!(env.get("FOO"), Some(&"bar".to_owned()));
        assert_eq!(env.get("PATH"), Some(&"/custom/bin:".to_owned()));
        // RunStep bind mounts: parent mkdir for nested inputs only.
        assert!(contents.contains("RUN mkdir -p web\n"));
        assert!(contents.contains(
            "RUN \\\n  --mount=type=bind,source=package.json,target=package.json \\\n  --mount=type=bind,source=web/app.json,target=web/app.json \\\n  npm install\n"
        ));
        assert!(contents.contains("COPY \\\n  --exclude=node_modules \\\n  . /app\n"));
        // Asset copy is inlined as base64.
        let expected_b64 = b64(b"hi");
        assert!(contents.contains(&format!(
            "RUN echo '{expected_b64}' | base64 -d > /app/cfg.ini\n"
        )));
        let motd_b64 = b64(b"hello");
        assert!(contents.contains(&format!(
            "RUN mkdir -p /etc && echo '{motd_b64}' | base64 -d > /etc/motd\n"
        )));
        assert!(contents.ends_with("\nFROM scratch\nCOPY --from=build /app /app\n"));
    }

    #[test]
    fn test_dockerfile_asset_errors() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("assets").join("dir")).unwrap();
        let backend = backend(tmp.path());
        let mut env = IndexMap::new();

        let missing = backend.dockerfile_contents(
            &mut env,
            &[],
            &[Step::Copy(CopyStep {
                source: "missing.txt".to_owned(),
                target: "/x".to_owned(),
                ignore: None,
                base: "assets".to_owned(),
            })],
        );
        assert_eq!(
            missing.unwrap_err().to_string(),
            "Asset missing.txt does not exist"
        );

        let dir = backend.dockerfile_contents(
            &mut env,
            &[],
            &[Step::Copy(CopyStep {
                source: "dir".to_owned(),
                target: "/x".to_owned(),
                ignore: None,
                base: "assets".to_owned(),
            })],
        );
        assert_eq!(
            dir.unwrap_err().to_string(),
            "Asset dir is a directory, anybuild doesn't currently support coppying assets directories inside Docker"
        );
    }

    #[test]
    fn test_download_copy_uses_add() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = backend(tmp.path());
        let mut env = IndexMap::new();
        let contents = backend
            .dockerfile_contents(
                &mut env,
                &[],
                &[Step::Copy(CopyStep {
                    source: "https://example.com/x.tar.gz".to_owned(),
                    target: "/tmp/x.tar.gz".to_owned(),
                    ignore: None,
                    base: "source".to_owned(),
                })],
            )
            .unwrap();
        assert!(contents.contains("ADD https://example.com/x.tar.gz /tmp/x.tar.gz\n"));
    }
}
