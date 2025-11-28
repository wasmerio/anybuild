use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::process::Command;

use camino::Utf8PathBuf;
use indexmap::IndexMap;
use semver::VersionReq;
use shell_words::split;
use wasmer_config::app::{AppConfigV1, AppVolume};
use wasmer_config::package::{
    Command as WasmerCommand, CommandAnnotations, CommandV2, Manifest, ModuleReference, Package as WasmerPackage,
};
use wasmer_config::package::PackageSource;

use crate::Result;
use crate::builder::Builder;
use crate::model::{Mount, Package, PrepareStep, Serve, Step};

/// Wasmer builder wraps another builder and emits wasmer.toml/app.yaml.
pub struct WasmerBuilder {
    pub inner: Box<dyn Builder>,
    pub workspace_dir: Utf8PathBuf,
    pub wasmer_dir: Utf8PathBuf,
    pub bin: String,
}

impl WasmerBuilder {
    pub fn new(inner: Box<dyn Builder>, workspace_dir: Utf8PathBuf) -> Result<Self> {
        let workspace_dir = if workspace_dir.is_absolute() {
            workspace_dir
        } else {
            let abs = std::fs::canonicalize(workspace_dir.as_str())?;
            Utf8PathBuf::from_path_buf(abs)
                .map_err(|_| anyhow::anyhow!("Workspace directory is not valid UTF-8"))?
        };
        let wasmer_dir = workspace_dir.join(".shipit/wasmer");
        Ok(Self {
            inner,
            workspace_dir,
            wasmer_dir,
            bin: "wasmer".to_string(),
        })
    }
}

impl Builder for WasmerBuilder {
    fn build(
        &mut self,
        env: &BTreeMap<String, String>,
        mounts: &[Mount],
        steps: &[Step],
    ) -> Result<()> {
        self.inner.build(env, mounts, steps)
    }

    fn build_prepare(&mut self, serve: &Serve) -> Result<()> {
        self.inner.build_prepare(serve)
    }

    fn prepare(&mut self, env: &BTreeMap<String, String>, prepare: &[PrepareStep]) -> Result<()> {
        self.inner.prepare(env, prepare)
    }

    fn build_serve(&mut self, serve: &Serve) -> Result<()> {
        // Ensure inner builder materializes serve artifacts (copies into serve mounts).
        self.inner.build_serve(serve)?;
        fs::create_dir_all(&self.wasmer_dir)?;

        // Map dependencies to Wasmer package versions.
        let (dependencies, binary_aliases) = map_dependencies(&serve.deps, serve.prepare.is_some())?;

        // Build command definitions.
        let mut commands = Vec::new();
        // Resolve base serve cwd relative to the app mount if needed.
        let app_mount = serve
            .mounts
            .as_ref()
            .and_then(|ms| ms.iter().find(|m| m.name == "app"));
        let app_serve_path = app_mount
            .map(|m| m.serve_path.to_string())
            .unwrap_or_else(|| "/app".to_string());

        for (name, line) in &serve.commands {
            let tokens = split(line)
                .map_err(|e| anyhow::anyhow!("Failed to parse command {name}: {e}"))?;
            if tokens.is_empty() {
                continue;
            }
            let program = &tokens[0];
            let (dep_name, module) = binary_aliases.get(program).cloned().ok_or_else(|| {
                anyhow::anyhow!("No Wasmer mapping found for command program {program}")
            })?;

            let mut env_pairs: Vec<String> = Vec::new();
            // Dependency-level env
            if let Some(env) = dependency_env(program) {
                env_pairs.extend(env);
            }
            if let Some(env) = &serve.env {
                env_pairs.extend(env.iter().map(|(k, v)| format!("{k}={v}")));
            }
            // Provide PORT default for static web server if not set.
            if !env_pairs.iter().any(|e| e.starts_with("PORT=")) {
                if let Some(port) = self.getenv("PORT") {
                    env_pairs.push(format!("PORT={port}"));
                } else {
                    env_pairs.push("PORT=8080".to_string());
                }
            }
            let port_value = env_pairs
                .iter()
                .find_map(|e| e.strip_prefix("PORT=").map(|v| v.to_string()))
                .unwrap_or_else(|| "8080".to_string());

            let mut wasi = toml::value::Table::new();
            if let Some(cwd) = &serve.cwd {
                let cwd_path = Utf8PathBuf::from(cwd);
                let resolved = if cwd_path.is_absolute() {
                    cwd.clone()
                } else if cwd == "app" || cwd == "." {
                    app_serve_path.clone()
                } else {
                    format!("{}/{}", app_serve_path.trim_end_matches('/'), cwd)
                };
                wasi.insert("cwd".into(), toml::Value::String(resolved));
            } else {
                wasi.insert("cwd".into(), toml::Value::String(app_serve_path.clone()));
            }
            if tokens.len() > 1 {
                wasi.insert(
                    "main-args".into(),
                    toml::Value::Array(
                        tokens[1..]
                            .iter()
                            .map(|s| toml::Value::String(resolve_arg_placeholders(
                                s,
                                &port_value
                            )))
                            .collect(),
                    ),
                );
            }
            if !env_pairs.is_empty() {
                wasi.insert(
                    "env".into(),
                    toml::Value::Array(
                        env_pairs
                            .into_iter()
                            .map(|s| toml::Value::String(s))
                            .collect(),
                    ),
                );
            }
            let mut annotations = toml::value::Table::new();
            annotations.insert("wasi".into(), toml::Value::Table(wasi));

            commands.push(WasmerCommand::V2(CommandV2 {
                name: name.clone(),
                module: ModuleReference::Dependency {
                    dependency: dep_name.clone(),
                    module,
                },
                runner: "wasi".to_string(),
                annotations: Some(CommandAnnotations::Raw(toml::Value::Table(annotations))),
            }));
        }

        // Filesystem mapping guest -> host (inner serve paths).
        let mut fs_map: IndexMap<String, Utf8PathBuf> = IndexMap::new();
        if let Some(mounts) = &serve.mounts {
            for mount in mounts {
                let inner_path = self.inner.get_serve_mount_path(&mount.name);
                fs_map.insert(mount.serve_path.to_string(), inner_path);
            }
        }

        // Build manifest.
        let entrypoint = if serve.commands.is_empty() {
            None
        } else {
            Some("start".to_string())
        };
        let mut manifest = Manifest::new_empty();
        let mut package = WasmerPackage::new_empty();
        package.entrypoint = entrypoint;
        manifest.package = Some(package);
        manifest.dependencies = dependencies;
        manifest.fs = fs_map
            .into_iter()
            .map(|(k, v)| (k, v.into_std_path_buf()))
            .collect();
        manifest.commands = commands;

        let manifest_toml = toml::to_string_pretty(&manifest)?;
        fs::write(self.wasmer_dir.join("wasmer.toml"), manifest_toml)?;

        // Minimal app.yaml pointing at the local package.
        let app_config = AppConfigV1 {
            name: None,
            app_id: None,
            owner: None,
            package: PackageSource::Path(".".to_string()),
            domains: None,
            locality: None,
            env: serve
                .env
                .clone()
                .map(|env| env.into_iter().collect::<IndexMap<_, _>>())
                .unwrap_or_default(),
            cli_args: None,
            capabilities: None,
            scheduled_tasks: None,
            volumes: serve.volumes.as_ref().map(|vols| {
                vols.iter()
                    .map(|v| AppVolume {
                        name: v.name.clone(),
                        mount: v.serve_path.to_string(),
                    })
                    .collect()
            }),
            health_checks: None,
            debug: None,
            scaling: None,
            redirect: None,
            jobs: None,
            extra: Default::default(),
        };
        let app_yaml = app_config.to_yaml()?;
        fs::write(self.wasmer_dir.join("app.yaml"), app_yaml)?;

        Ok(())
    }

    fn finalize_build(&mut self, serve: &Serve) -> Result<()> {
        self.inner.finalize_build(serve)
    }

    fn getenv(&self, name: &str) -> Option<String> {
        self.inner.getenv(name)
    }

    fn run_serve_command(&mut self, command: &str) -> Result<()> {
        let bin = self.bin.clone();
        let mut args = Vec::new();
        args.push("run".to_string());
        args.push(self.wasmer_dir.to_string());
        args.push(format!("--command={command}"));
        self.run_command(&bin, Some(&args))
    }

    fn run_command(&mut self, command: &str, extra_args: Option<&[String]>) -> Result<()> {
        tracing::info!(command, ?extra_args, "invoking wasmer");
        let mut cmd = Command::new(command);
        if let Some(args) = extra_args {
            cmd.args(args);
        }
        cmd.current_dir(&self.wasmer_dir);
        let status = cmd.status()?;
        if !status.success() {
            return Err(anyhow::anyhow!("Command {} failed with {}", command, status));
        }
        Ok(())
    }

    fn get_build_mount_path(&self, name: &str) -> Utf8PathBuf {
        // Wasmer uses different serve paths; build paths mirror inner builder.
        self.inner.get_build_mount_path(name)
    }

    fn get_serve_mount_path(&self, name: &str) -> Utf8PathBuf {
        // Serve mounts map to /app or /opt/<name> inside Wasmer
        match name {
            "app" => Utf8PathBuf::from("/app"),
            other => Utf8PathBuf::from(format!("/opt/{}", other)),
        }
    }
}

struct DependencyMapping {
    package: String,
    version_req: VersionReq,
    module: String,
    aliases: BTreeSet<String>,
}

fn map_dependencies(
    deps: &[Package],
    needs_bash: bool,
) -> Result<(IndexMap<String, VersionReq>, BTreeMap<String, (String, String)>)> {
    let mut deps = deps.to_vec();
    if needs_bash && deps.iter().all(|d| d.name != "bash") {
        deps.push(Package {
            name: "bash".to_string(),
            version: Some("8.3".to_string()),
            architecture: None,
        });
    }

    let mut manifest_deps: IndexMap<String, VersionReq> = IndexMap::new();
    let mut binaries: BTreeMap<String, (String, String)> = BTreeMap::new();

    for dep in deps {
        let mapping = map_dependency(&dep)?;
        manifest_deps.insert(mapping.package.clone(), mapping.version_req);
        for alias in mapping.aliases {
            binaries.insert(alias, (mapping.package.clone(), mapping.module.clone()));
        }
    }

    Ok((manifest_deps, binaries))
}

fn map_dependency(dep: &Package) -> Result<DependencyMapping> {
    match dep.name.as_str() {
        "static-web-server" => {
            let version = dep.version.as_deref().unwrap_or("latest");
            let version_req = match version {
                "latest" | "2.38.0" | "0.1" => VersionReq::parse("=1.1.0")?,
                other => {
                    let req = format!("={other}");
                    VersionReq::parse(&req)?
                }
            };
            let mut aliases = BTreeSet::new();
            aliases.insert("static-web-server".to_string());
            aliases.insert("webserver".to_string());
            Ok(DependencyMapping {
                package: "wasmer/static-web-server".to_string(),
                version_req,
                module: "webserver".to_string(),
                aliases,
            })
        }
        "bash" => {
            let version_req = VersionReq::parse("=1.0.24")?;
            let mut aliases = BTreeSet::new();
            aliases.insert("bash".to_string());
            aliases.insert("sh".to_string());
            Ok(DependencyMapping {
                package: "wasmer/bash".to_string(),
                version_req,
                module: "bash".to_string(),
                aliases,
            })
        }
        other => Err(anyhow::anyhow!(
            "Dependency {other} not available for Wasmer packaging yet"
        )),
    }
}

fn dependency_env(program: &str) -> Option<Vec<String>> {
    if program == "python" {
        return Some(vec![
            "PYTHONEXECUTABLE=/bin/python".to_string(),
            "PYTHONDONTWRITEBYTECODE=1".to_string(),
        ]);
    }
    None
}

fn resolve_arg_placeholders(arg: &str, port_value: &str) -> String {
    let mut out = arg.to_string();
    for pat in ["${PORT:-8080}", "${PORT}", "$PORT"] {
        if out.contains(pat) {
            out = out.replace(pat, port_value);
        }
    }
    out
}
