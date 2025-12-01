use std::collections::BTreeMap;

use anyhow::Context;
use camino::Utf8PathBuf;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use crate::Result;
use crate::assets;
use crate::builder::Builder;
use crate::model::{CopyBase, Mount, PrepareStep, Serve, Step};

/// Local builder executes steps on the host and writes artifacts to `.shipit/local`.
pub struct LocalBuilder {
    pub src_dir: Utf8PathBuf,
    pub build_dir: Utf8PathBuf,
    pub serve_dir: Utf8PathBuf,
}

impl LocalBuilder {
    pub fn new(src_dir: Utf8PathBuf) -> crate::Result<Self> {
        let src_dir = if src_dir.is_absolute() {
            src_dir
        } else {
            let abs = std::fs::canonicalize(src_dir.as_str())?;
            Utf8PathBuf::from_path_buf(abs)
                .map_err(|_| anyhow::anyhow!("Source directory is not valid UTF-8"))?
        };
        let build_dir = src_dir.join(".shipit/local/build");
        let serve_dir = src_dir.join(".shipit/local/serve");
        Ok(Self {
            src_dir,
            build_dir,
            serve_dir,
        })
    }
}

impl Builder for LocalBuilder {
    fn build(
        &mut self,
        _env: &BTreeMap<String, String>,
        _mounts: &[Mount],
        steps: &[Step],
    ) -> Result<()> {
        std::fs::create_dir_all(&self.build_dir)?;
        let mut cwd = self.build_dir.clone();
        let mut env_overlay: BTreeMap<String, String> = _env.clone();
        // Seed PATH with host PATH so path/prepend mutations work.
        if !env_overlay.contains_key("PATH") {
            if let Ok(path) = std::env::var("PATH") {
                env_overlay.insert("PATH".to_string(), path);
            }
        }
        for step in steps {
            match step {
                Step::Workdir(w) => {
                    cwd = if w.path.is_absolute() {
                        w.path.clone()
                    } else {
                        cwd.join(&w.path)
                    };
                    std::fs::create_dir_all(&cwd)?;
                }
                Step::Copy(c) => {
                    // Resolve the target path. If the provided target is absolute, use it as-is.
                    // Otherwise, resolve relative targets against the current working dir.
                    let target = if Utf8PathBuf::from(&c.target).is_absolute() {
                        Utf8PathBuf::from(&c.target)
                    } else {
                        cwd.join(&c.target)
                    };

                    if matches!(c.base, CopyBase::Assets) {
                        if let Some(data) = assets::get_asset(&c.source) {
                            if let Some(parent) = target.parent() {
                                std::fs::create_dir_all(parent)?;
                            }
                            std::fs::write(&target, data)?;
                        } else {
                            return Err(anyhow::anyhow!("Asset {} not found", c.source));
                        }
                    } else {
                        // For source-based copies, normalize the source resolution:
                        // - A source of "." refers to the project source directory (`self.src_dir`).
                        // - An absolute source is used as provided.
                        // - A relative source is resolved against the project source directory.
                        let source = if c.source == "." {
                            self.src_dir.clone()
                        } else if Utf8PathBuf::from(&c.source).is_absolute() {
                            Utf8PathBuf::from(&c.source)
                        } else {
                            self.src_dir.join(&c.source)
                        };

                        copy_path(&source, &target, &c.ignore)?;
                    }
                }
                Step::Env(e) => {
                    for (k, v) in &e.variables {
                        env_overlay.insert(k.clone(), v.clone());
                    }
                }
                Step::Path(p) => {
                    let current = env_overlay
                        .get("PATH")
                        .cloned()
                        .or_else(|| std::env::var("PATH").ok())
                        .unwrap_or_default();
                    let mut new_path = p.path.clone();
                    if !current.is_empty() {
                        new_path.push(':');
                        new_path.push_str(&current);
                    }
                    env_overlay.insert("PATH".to_string(), new_path);
                }
                Step::Use(_) => {
                    // Local builder assumes dependencies already available on host.
                }
                Step::Run(r) => {
                    // If the run step declares inputs, copy them from the project
                    // source into the current working dir so the command can
                    // access them in `cwd`.
                    if !r.inputs.is_empty() {
                        for input in &r.inputs {
                            let src = self.src_dir.join(input);
                            let dest = cwd.join(input);
                            if src.exists() {
                                if let Some(parent) = dest.parent() {
                                    std::fs::create_dir_all(parent)?;
                                }
                                if src.is_dir() {
                                    // Reuse copy_path to copy directories (preserves layout).
                                    copy_path(&src, &dest, &[])?;
                                } else {
                                    std::fs::copy(src.as_std_path(), dest.as_std_path())?;
                                }
                            }
                        }
                    }
                    run_shell(&cwd, &env_overlay, &r.command)?;
                }
            }
        }
        Ok(())
    }

    fn build_prepare(&mut self, _serve: &Serve) -> Result<()> {
        Ok(())
    }

    fn prepare(&mut self, _env: &BTreeMap<String, String>, _prepare: &[PrepareStep]) -> Result<()> {
        let mut cwd = self.build_dir.clone();
        let mut env_overlay: BTreeMap<String, String> = _env.clone();
        if !env_overlay.contains_key("PATH") {
            if let Ok(path) = std::env::var("PATH") {
                env_overlay.insert("PATH".to_string(), path);
            }
        }
        for step in _prepare {
            match step {
                crate::model::Step::Run(run) => {
                    run_shell(&cwd, &env_overlay, &run.command)?;
                }
                crate::model::Step::Workdir(workdir) => {
                    cwd = cwd.join(&workdir.path);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn build_serve(&mut self, _serve: &Serve) -> Result<()> {
        // Ensure serve mount has contents from build output.
        let app_build = self.build_dir.join("app");
        let app_serve = self.serve_dir.join("app");
        if app_build.exists() {
            copy_path(&app_build, &app_serve, &[])?;
        }

        let bin_dir = self.serve_dir.join("bin");
        std::fs::create_dir_all(&bin_dir)?;
        // For static builds, write a start script derived from serve commands/cwd/env.
        let start_path = bin_dir.join("start");
        let mut script = String::from("#!/usr/bin/env bash\nset -euo pipefail\n");
        if let Some(env) = &_serve.env {
            for (k, v) in env {
                script.push_str(&format!("export {k}=\"{v}\"\n"));
            }
        }
        if let Some(cwd) = &_serve.cwd {
            // If cwd is relative, anchor it at the serve root.
            if Utf8PathBuf::from(cwd).is_absolute() {
                script.push_str(&format!("cd \"{cwd}\"\n"));
            } else {
                script.push_str("cd \"$(dirname \"$0\")/../");
                script.push_str(cwd);
                script.push_str("\"\n");
            }
        } else {
            script.push_str("cd \"$(dirname \"$0\")/../app\"\n");
        }
        let command = _serve.commands.get("start").cloned().unwrap_or_else(|| {
            "static-web-server --root=. --log-level=info --port=${PORT:-8080}".to_string()
        });
        script.push_str(&format!("exec {command}\n"));
        std::fs::write(&start_path, script)?;
        std::fs::set_permissions(&start_path, std::fs::Permissions::from_mode(0o755))?;
        Ok(())
    }

    fn finalize_build(&mut self, _serve: &Serve) -> Result<()> {
        Ok(())
    }

    fn getenv(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn run_serve_command(&mut self, _command: &str) -> Result<()> {
        let bin = self.serve_dir.join("bin").join(_command);
        let status = Command::new(bin.as_std_path())
            .envs(std::env::vars())
            .status()?;
        if !status.success() {
            return Err(anyhow::anyhow!("Serve command exited with {}", status));
        }
        Ok(())
    }

    fn run_command(&mut self, _command: &str, _extra_args: Option<&[String]>) -> Result<()> {
        let mut cmd = Command::new(_command);
        if let Some(args) = _extra_args {
            cmd.args(args);
        }
        cmd.current_dir(&self.src_dir);
        cmd.envs(std::env::vars());
        let status = cmd.status()?;
        if !status.success() {
            return Err(anyhow::anyhow!(
                "Command {} failed with {}",
                _command,
                status
            ));
        }
        Ok(())
    }

    fn get_build_mount_path(&self, name: &str) -> Utf8PathBuf {
        match name {
            "app" => self.build_dir.join("app"),
            other => self.build_dir.join(other),
        }
    }

    fn get_serve_mount_path(&self, name: &str) -> Utf8PathBuf {
        match name {
            "app" => self.serve_dir.join("app"),
            other => self.serve_dir.join(other),
        }
    }

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn copy_path(source: &Utf8PathBuf, target: &Utf8PathBuf, ignore: &[String]) -> Result<()> {
    let mut ignore_set: std::collections::HashSet<String> = ignore.iter().cloned().collect();
    ignore_set.insert(".shipit".to_string());
    ignore_set.insert("Shipit".to_string());

    // Normalize/resolve absolute paths for consistent behavior across builders.
    let mut src = source.clone();
    if !src.is_absolute() {
        // Try to canonicalize if the path exists; otherwise resolve relative to current dir.
        if let Ok(canon) = std::fs::canonicalize(src.as_std_path()) {
            src = Utf8PathBuf::from_path_buf(canon)
                .map_err(|_| anyhow::anyhow!("Source path is not valid UTF-8"))?;
        } else {
            let cwd = std::env::current_dir()?;
            let abs = cwd.join(src.as_str());
            src = Utf8PathBuf::from_path_buf(abs)
                .map_err(|_| anyhow::anyhow!("Source path is not valid UTF-8"))?;
        }
    }

    let mut dst = target.clone();
    if !dst.is_absolute() {
        let cwd = std::env::current_dir()?;
        let abs = cwd.join(dst.as_str());
        dst = Utf8PathBuf::from_path_buf(abs)
            .map_err(|_| anyhow::anyhow!("Target path is not valid UTF-8"))?;
    }

    // If the source is a file, copy it directly to the target path.
    if src.is_file() {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src.as_std_path(), dst.as_std_path())
            .with_context(|| format!("Failed to copy {} to {}", src, dst))?;
        return Ok(());
    }

    // Walk the source tree and copy entries into the destination tree,
    // preserving relative layout from the source root.
    for entry in walkdir::WalkDir::new(src.as_std_path()) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(src.as_std_path()).unwrap();
        let name = rel
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        if ignore_set.contains(&name) {
            continue;
        }
        let rel_str = rel.to_string_lossy();
        if rel_str.starts_with(".shipit")
            || rel_str.starts_with("Shipit")
            || rel_str.contains("/.shipit/")
            || rel_str.contains("/Shipit/")
        {
            continue;
        }
        let rel_utf8 = Utf8PathBuf::from_path_buf(rel.to_path_buf())
            .map_err(|_| anyhow::anyhow!("Non-UTF8 path encountered during copy"))?;
        let dest = dst.join(rel_utf8);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dest)
                .with_context(|| format!("Failed to create directory {}", dest))?;
        } else {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(&parent)
                    .with_context(|| format!("Failed to create parent directory for {}", dest))?;
            }
            std::fs::copy(entry.path(), dest.as_std_path()).with_context(|| {
                format!("Failed to copy {} to {}", entry.path().display(), dest)
            })?;
        }
    }
    Ok(())
}

fn run_shell(
    cwd: &Utf8PathBuf,
    env_overlay: &BTreeMap<String, String>,
    command: &str,
) -> Result<()> {
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(command);
    cmd.current_dir(cwd);
    // Base environment first, then overlay.
    cmd.envs(std::env::vars());
    cmd.envs(env_overlay.iter().map(|(k, v)| (k, v)));
    let status = cmd.status()?;
    if !status.success() {
        return Err(anyhow::anyhow!(
            "Command `{}` failed with {}",
            command,
            status
        ));
    }
    Ok(())
}
