use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::process::Command;

use camino::Utf8PathBuf;
use indexmap::IndexMap;
use semver::VersionReq;

use sha2::{Digest, Sha256};
use shell_words::split;
use wasmer_config::app::{AppConfigV1, AppVolume};
use wasmer_config::package::PackageSource;
use wasmer_config::package::{
    Command as WasmerCommand, CommandAnnotations, CommandV2, Manifest, ModuleReference,
    Package as WasmerPackage,
};

use crate::Result;
use crate::builder::Builder;
use crate::model::{Mount, Package, PrepareStep, Serve, Step};

/// Wasmer builder wraps another builder and emits wasmer.toml/app.yaml.
pub struct WasmerBuilder {
    pub inner: Box<dyn Builder>,
    pub workspace_dir: Utf8PathBuf,
    pub wasmer_dir: Utf8PathBuf,
    pub bin: String,
    pub registry: Option<String>,
    pub token: Option<String>,
}

impl WasmerBuilder {
    pub fn new(
        inner: Box<dyn Builder>,
        workspace_dir: Utf8PathBuf,
        registry: Option<String>,
        token: Option<String>,
    ) -> Result<Self> {
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
            registry,
            token,
        })
    }
}

// Helper: normalize legacy placeholder patterns seen in Shipit commands.
// It replaces common PORT placeholders with a concrete value and ensures the
// app path is substituted for relative references to `app`.
fn normalize_command_line(line: &str, app_path: &str, port_value: &str) -> String {
    let mut out = line.to_string();
    // Replace PORT forms
    for pat in ["${PORT:-8080}", "${PORT}", "$PORT"] {
        if out.contains(pat) {
            out = out.replace(pat, port_value);
        }
    }
    // Replace occurrences of {app} or ./app with app_path when present.
    out = out.replace("{app}", app_path);
    out = out.replace("./app", app_path);
    out
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
        let prepare_dir = self.wasmer_dir.join("prepare");
        fs::create_dir_all(&prepare_dir)?;
        let mut env_lines = Vec::new();
        for dep in &serve.deps {
            if let Some(env) = dependency_env(&dep.name) {
                env_lines.extend(env);
            }
        }
        if let Some(env) = &serve.env {
            for (k, v) in env {
                env_lines.push(format!("export {}={}", k, v));
            }
        }
        let env_part = if env_lines.is_empty() {
            "".to_string()
        } else {
            env_lines.join("\n") + "\n"
        };
        let mut commands = Vec::new();
        if let Some(cwd) = &serve.cwd {
            commands.push(format!("cd {}", cwd));
        }
        if let Some(prepare) = &serve.prepare {
            for step in prepare {
                match step {
                    Step::Run(run_step) => commands.push(run_step.command.clone()),
                    Step::Workdir(workdir_step) => {
                        commands.push(format!("cd {}", workdir_step.path))
                    }
                    _ => {}
                }
            }
        }
        let body = commands.join("\n");
        let script = format!("#!/bin/bash\n\n{}{}", env_part, body);
        fs::write(prepare_dir.join("prepare.sh"), &script)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(prepare_dir.join("prepare.sh"))?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(prepare_dir.join("prepare.sh"), perms)?;
        }
        Ok(())
    }

    fn prepare(&mut self, _env: &BTreeMap<String, String>, _prepare: &[PrepareStep]) -> Result<()> {
        let prepare_dir = self.wasmer_dir.join("prepare");
        let bin = self.bin.clone();
        let mut args = vec![
            "run".to_string(),
            self.wasmer_dir.to_string(),
            "--net".to_string(),
            "--command=bash".to_string(),
            format!("--mapdir=/prepare:{}", prepare_dir),
            "--".to_string(),
            "/prepare/prepare.sh".to_string(),
        ];
        if let Some(ref reg) = self.registry {
            args.insert(1, format!("--registry={}", reg));
        }
        self.run_command(&bin, Some(&args))
    }

    fn build_serve(&mut self, serve: &Serve) -> Result<()> {
        // Ensure inner builder materializes serve artifacts (copies into serve mounts).
        self.inner.build_serve(serve)?;
        fs::create_dir_all(&self.wasmer_dir)?;

        // Map dependencies to Wasmer package versions.
        let (dependencies, binary_aliases) =
            map_dependencies(&serve.deps, serve.prepare.is_some())?;

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
            // Normalize legacy format/placeholder patterns to concrete args before mapping.
            let normalized_line = normalize_command_line(line, &app_serve_path, "8080");
            let tokens = split(&normalized_line)
                .map_err(|e| anyhow::anyhow!("Failed to parse command {name}: {e}"))?;
            if tokens.is_empty() {
                continue;
            }
            let program = &tokens[0];

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
            let normalized_line = normalize_command_line(line, &app_serve_path, &port_value);
            let tokens = split(&normalized_line)
                .map_err(|e| anyhow::anyhow!("Failed to parse command {name}: {e}"))?;
            if tokens.is_empty() {
                continue;
            }
            let program = &tokens[0];
            let (dep_name, module) = binary_aliases.get(program).cloned().ok_or_else(|| {
                anyhow::anyhow!("No Wasmer mapping found for command program {program}")
            })?;

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
                            .map(|s| toml::Value::String(resolve_arg_placeholders(s, &port_value)))
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
            capabilities: None, // TODO: Implement capabilities mapping with correct types
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
            scaling: None, // TODO: Implement scaling with correct types
            redirect: None,
            jobs: None, // TODO: Implement jobs with correct types
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
        let mut args = vec![
            "run".to_string(),
            self.wasmer_dir.to_string(),
            "--net".to_string(),
            format!("--command={command}"),
        ];
        if let Some(ref reg) = self.registry {
            args.insert(1, format!("--registry={}", reg));
        }
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
            return Err(anyhow::anyhow!(
                "Command {} failed with {}",
                command,
                status
            ));
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

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl WasmerBuilder {
    /// Stub for deploy-config parity; to be implemented with real packaging.
    pub fn deploy_config(&mut self, path: &Utf8PathBuf) -> Result<()> {
        let package_webc_path = self.wasmer_dir.join("package.webc");
        let app_yaml_path = self.wasmer_dir.join("app.yaml");
        if let Some(parent) = package_webc_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bin = self.bin.clone();
        let wasmer_dir_str = self.wasmer_dir.to_string();
        let package_webc_path_str = package_webc_path.to_string();
        self.run_command(
            &bin,
            Some(&[
                "package".to_string(),
                "build".to_string(),
                wasmer_dir_str,
                "--out".to_string(),
                package_webc_path_str,
            ]),
        )?;
        let size = fs::metadata(&package_webc_path)?.len();
        let hash = {
            let data = fs::read(&package_webc_path)?;
            let mut hasher = Sha256::new();
            hasher.update(&data);
            format!("{:x}", hasher.finalize())
        };
        let config = serde_json::json!({
            "app_yaml_path": app_yaml_path,
            "package_webc_path": package_webc_path,
            "package_webc_size": size,
            "package_webc_sha256": hash,
        });
        fs::write(path, config.to_string())?;
        Ok(())
    }

    /// Stub for Wasmer deploy parity.
    pub fn deploy(&mut self, app_owner: Option<String>, app_name: Option<String>) -> Result<()> {
        let bin = self.bin.clone();
        let mut args = vec![
            "deploy".to_string(),
            "--publish-package".to_string(),
            "--dir".to_string(),
            self.wasmer_dir.to_string(),
            "--non-interactive".to_string(),
        ];
        if let Some(ref reg) = self.registry {
            args.push("--registry".to_string());
            args.push(reg.clone());
        }
        if let Some(ref tok) = self.token {
            args.push("--token".to_string());
            args.push(tok.clone());
        }
        if let Some(owner) = app_owner {
            args.push("--owner".to_string());
            args.push(owner);
        }
        if let Some(name) = app_name {
            args.push("--app-name".to_string());
            args.push(name);
        }
        self.run_command(&bin, Some(&args))
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
) -> Result<(
    IndexMap<String, VersionReq>,
    BTreeMap<String, (String, String)>,
)> {
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
        "python" => {
            let version = dep.version.as_deref().unwrap_or("latest");
            let version_req = match version {
                "latest" | "3.13" => VersionReq::parse("=3.13.3")?,
                other => VersionReq::parse(&format!("={}", other))?,
            };
            let mut aliases = BTreeSet::new();
            aliases.insert("python".to_string());
            Ok(DependencyMapping {
                package: "python/python".to_string(),
                version_req,
                module: "python".to_string(),
                aliases,
            })
        }
        "pandoc" => {
            let version_req = VersionReq::parse("=0.0.1")?;
            let mut aliases = BTreeSet::new();
            aliases.insert("pandoc".to_string());
            Ok(DependencyMapping {
                package: "wasmer/pandoc".to_string(),
                version_req,
                module: "pandoc".to_string(),
                aliases,
            })
        }
        "ffmpeg" => {
            let version_req = VersionReq::parse("=1.0.5")?;
            let mut aliases = BTreeSet::new();
            aliases.insert("ffmpeg".to_string());
            Ok(DependencyMapping {
                package: "wasmer/ffmpeg".to_string(),
                version_req,
                module: "ffmpeg".to_string(),
                aliases,
            })
        }
        "php" => {
            let version = dep.version.as_deref().unwrap_or("latest");
            let arch = dep.architecture.as_deref().unwrap_or("64-bit");
            let package_base = match arch {
                "64-bit" => "php/php-64",
                "32-bit" => "php/php-32",
                _ => return Err(anyhow::anyhow!("Unsupported architecture {}", arch)),
            };
            let version_req = match version {
                "latest" | "8.3" => VersionReq::parse("=8.3.2102")?,
                "8.2" => VersionReq::parse("=8.2.2801")?,
                "8.1" => VersionReq::parse("=8.1.3201")?,
                "7.4" => VersionReq::parse("=7.4.3301")?,
                other => VersionReq::parse(&format!("={}", other))?,
            };
            let mut aliases = BTreeSet::new();
            aliases.insert("php".to_string());
            Ok(DependencyMapping {
                package: package_base.to_string(),
                version_req,
                module: "php".to_string(),
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
