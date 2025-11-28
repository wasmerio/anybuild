use std::collections::BTreeMap;

use camino::Utf8PathBuf;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use crate::Result;
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
                    let base = match c.base {
                        CopyBase::Source => self.src_dir.join(&c.source),
                        CopyBase::Assets => self.src_dir.join("src/shipit/assets").join(&c.source),
                    };
                    let target = cwd.join(&c.target);
                    copy_path(&base, &target, &c.ignore)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn build_prepare(&mut self, _serve: &Serve) -> Result<()> {
        todo!("LocalBuilder.build_prepare not yet implemented")
    }

    fn prepare(&mut self, _env: &BTreeMap<String, String>, _prepare: &[PrepareStep]) -> Result<()> {
        todo!("LocalBuilder.prepare not yet implemented")
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
        todo!("LocalBuilder.run_command not yet implemented")
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
}

fn copy_path(source: &Utf8PathBuf, target: &Utf8PathBuf, ignore: &[String]) -> Result<()> {
    let mut ignore_set: std::collections::HashSet<String> = ignore.iter().cloned().collect();
    ignore_set.insert(".shipit".to_string());
    ignore_set.insert("Shipit".to_string());

    if source.is_file() {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(source, target)?;
        return Ok(());
    }

    for entry in walkdir::WalkDir::new(source) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(source.as_std_path()).unwrap();
        let name = rel
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        if ignore_set.contains(&name) {
            continue;
        }
        let rel_utf8 = Utf8PathBuf::from_path_buf(rel.to_path_buf())
            .map_err(|_| anyhow::anyhow!("Non-UTF8 path encountered during copy"))?;
        let dest = target.join(rel_utf8);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dest)?;
        } else {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}
