//! Local build backend.
//!
//! Output strings are verbatim ports of the Python `console.print` calls
//! (rich markup renders as plain text when piped, so the markup tags are
//! dropped here). The Python console writes to stderr; so does this.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};
use indexmap::IndexMap;

use crate::common::paths::normalize_absolute;
use crate::operation::OperationContext;
use crate::plan::{Mount, Step};

use crate::build::BuildBackend;

use crate::build::report::{build_started, build_step};

fn shorten_local_build_paths(description: &str, build_path: &Path) -> String {
    let build_path = build_path.to_string_lossy();
    if build_path.is_empty() {
        return description.to_owned();
    }

    let mut output = String::with_capacity(description.len());
    let mut remainder = description;
    while let Some(index) = remainder.find(build_path.as_ref()) {
        output.push_str(&remainder[..index]);
        let after_path = &remainder[index + build_path.len()..];
        if let Some(after_separator) = after_path
            .strip_prefix('/')
            .or_else(|| after_path.strip_prefix('\\'))
        {
            output.push('/');
            remainder = after_separator;
        } else if after_path.is_empty()
            || after_path.starts_with([
                ' ', '\t', '\n', '\r', '\'', '"', ';', ':', '|', '&', ')', ']', '}', ',',
            ])
        {
            output.push('/');
            remainder = after_path;
        } else {
            output.push_str(build_path.as_ref());
            remainder = after_path;
        }
    }
    output.push_str(remainder);
    output
}

/// Port of `utils.py::download_file`.
pub fn download_file(url: &str, path: &Path) -> Result<()> {
    let response = ureq::get(url)
        .call()
        .with_context(|| format!("Failed to download {url}"))?;
    let mut bytes: Vec<u8> = Vec::new();
    use std::io::Read;
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .with_context(|| format!("Failed to read response body from {url}"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

/// `shutil.ignore_patterns(...)` semantics: fnmatch on entry basenames at
/// every directory level.
fn build_ignore_set(patterns: &[String]) -> Result<globset::GlobSet> {
    let mut builder = globset::GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(
            globset::Glob::new(pattern)
                .with_context(|| format!("Invalid ignore pattern: {pattern}"))?,
        );
    }
    Ok(builder.build()?)
}

/// Port of `shutil.copytree(source, target, dirs_exist_ok=True,
/// ignore=shutil.ignore_patterns(*patterns))`: recursive copy that skips
/// entries whose basename matches any pattern.
pub fn copy_tree_with_ignore(source: &Path, target: &Path, patterns: &[String]) -> Result<()> {
    let ignore = build_ignore_set(patterns)?;
    copy_tree_inner(source, target, &ignore)
}

fn copy_tree_inner(source: &Path, target: &Path, ignore: &globset::GlobSet) -> Result<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)
        .with_context(|| format!("Failed to read directory {}", source.display()))?
    {
        let entry = entry?;
        let name = entry.file_name();
        if ignore.is_match(Path::new(&name)) {
            continue;
        }
        let src = entry.path();
        let dst = target.join(&name);
        // Follows symlinks, like shutil.copytree(symlinks=False).
        let metadata =
            std::fs::metadata(&src).with_context(|| format!("Failed to stat {}", src.display()))?;
        if metadata.is_dir() {
            copy_tree_inner(&src, &dst, ignore)?;
        } else {
            std::fs::copy(&src, &dst)
                .with_context(|| format!("Failed to copy {}", src.display()))?;
        }
    }
    Ok(())
}

/// `shutil.copytree(..., dirs_exist_ok=True)` without ignore patterns.
pub fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    copy_tree_with_ignore(source, target, &[])
}

/// Python `base / part`: empty parts are no-ops and absolute parts replace
/// the base; the result is normalized (no trailing separator).
fn pythonic_join(base: &Path, part: &str) -> PathBuf {
    if part.is_empty() {
        return base.to_path_buf();
    }
    let part_path = Path::new(part);
    if part_path.is_absolute() {
        return part_path.to_path_buf();
    }
    base.join(part_path)
}

/// `shutil.which(program, path=PATH)`.
fn which(program: &str, search_paths: &[PathBuf]) -> Option<PathBuf> {
    fn is_executable_file(path: &Path) -> bool {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(path)
                .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
        }
        #[cfg(not(unix))]
        {
            path.is_file()
        }
    }
    if program.contains('/') {
        let candidate = PathBuf::from(program);
        let candidate = if candidate.is_absolute() {
            candidate
        } else {
            std::env::current_dir().ok()?.join(candidate)
        };
        return is_executable_file(&candidate).then_some(candidate);
    }
    for dir in search_paths {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let candidate = dir.join(program);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn display_environment_variables(variables: &IndexMap<String, String>) -> String {
    variables
        .iter()
        .map(|(name, value)| {
            let value = serde_json::to_string(value)
                .expect("serializing an environment variable value cannot fail");
            format!("{name}={value}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Port of `builders/local.py::LocalBuildBackend`.
pub struct LocalBuildBackend {
    pub src_dir: PathBuf,
    pub assets_path: PathBuf,
    pub anybuild_dir: PathBuf,
    pub local_path: PathBuf,
    pub build_path: PathBuf,
    workdir: PathBuf,
    runtime_path: Option<String>,
    operation: OperationContext,
}

impl LocalBuildBackend {
    pub fn new(
        src_dir: PathBuf,
        assets_path: PathBuf,
        anybuild_dir: Option<PathBuf>,
        operation: OperationContext,
    ) -> Self {
        let anybuild_dir = anybuild_dir.unwrap_or_else(|| src_dir.join(".anybuild"));
        let local_path = anybuild_dir.join("local");
        let build_path = local_path.join("build");
        Self {
            src_dir,
            assets_path,
            anybuild_dir,
            local_path,
            workdir: build_path.clone(),
            build_path,
            runtime_path: None,
            operation,
        }
    }

    pub fn get_mount_path(&self, name: &str) -> PathBuf {
        if name == "app" {
            self.build_path.join("app")
        } else {
            self.build_path.join("opt").join(name)
        }
    }

    fn report_build_step(&self, description: impl Into<String>) {
        let description = description.into();
        build_step(
            &self.operation,
            shorten_local_build_paths(&description, &self.build_path),
        );
    }

    fn execute_step(&mut self, step: &Step, env: &mut IndexMap<String, String>) -> Result<()> {
        let build_path = self.workdir.clone();
        match step {
            Step::Use(step) => {
                let deps = step
                    .dependencies
                    .iter()
                    .map(|dep| dep.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                self.report_build_step(format!("Using dependencies: {deps}"));
            }
            Step::Workdir(step) => {
                self.report_build_step(format!("Working in {}", step.path.display()));
                self.workdir = step.path.clone();
                std::fs::create_dir_all(&step.path)?;
            }
            Step::Run(step) => {
                let mut extra = String::new();
                let inputs = step.inputs.as_deref().unwrap_or(&[]);
                if !inputs.is_empty() {
                    for input in inputs {
                        let source = pythonic_join(&self.src_dir, input);
                        let target = pythonic_join(&build_path, input);
                        self.report_build_step(format!(
                            "Copying {} to {}",
                            input,
                            target.display()
                        ));
                        if let Some(parent) = target.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        if source.is_dir() {
                            copy_tree(&source, &target)?;
                        } else {
                            std::fs::copy(&source, &target)
                                .with_context(|| format!("Failed to copy {}", source.display()))?;
                        }
                    }
                    let all_inputs = inputs.join(", ");
                    extra = format!(" # using {all_inputs}");
                }
                self.report_build_step(format!("$ {}{}", step.command, extra));
                let command_line = &step.command;
                let parts = shell_words::split(command_line)
                    .map_err(|e| anyhow!("Failed to parse command {command_line:?}: {e}"))?;
                let program = parts
                    .first()
                    .ok_or_else(|| anyhow!("Program is not installed: "))?;
                let empty = String::new();
                let env_path = env.get("PATH").unwrap_or(&empty);
                // Plan env values are POSIX (':'-separated); the host PATH
                // appended after them uses the platform separator.
                let mut search_paths: Vec<PathBuf> = env_path
                    .split(':')
                    .map(|path| pythonic_join(&build_path, path))
                    .collect();
                search_paths.extend(std::env::split_paths(
                    &self.operation.environment_var("PATH").unwrap_or_default(),
                ));
                if which(program, &search_paths).is_none() {
                    bail!("Program is not installed: {program}");
                }
                // The command runs under bash, so its PATH is ':'-joined
                // regardless of host platform.
                let full_path = search_paths
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(":");
                let mut command = Command::new("bash");
                self.operation.prepare_command(&mut command);
                command
                    .arg("-c")
                    .arg(command_line)
                    .env_clear()
                    .envs(env.iter())
                    .env("PATH", &full_path)
                    .current_dir(&build_path);
                let status = self
                    .operation
                    .command_status(&mut command)
                    .with_context(|| format!("Failed to run: {command_line}"))?;
                if !status.success() {
                    bail!(
                        "Command failed with exit code {}: {command_line}",
                        status.code().unwrap_or(-1)
                    );
                }
            }
            Step::Copy(step) => {
                let mut ignore_extra = String::new();
                let step_ignore = step.ignore.as_deref().unwrap_or(&[]);
                if !step_ignore.is_empty() {
                    ignore_extra = format!(" # ignoring {}", step_ignore.join(", "));
                }
                let mut ignore_matches: Vec<String> = step_ignore.to_vec();
                ignore_matches.push(".anybuild".to_owned());
                ignore_matches.push("Anybuild".to_owned());

                if step.is_download() {
                    self.report_build_step(format!(
                        "Download from {} to {}",
                        step.source, step.target
                    ));
                    download_file(&step.source, &pythonic_join(&build_path, &step.target))?;
                } else {
                    let base = match step.base.as_str() {
                        "source" => &self.src_dir,
                        "assets" => &self.assets_path,
                        other => bail!("Unknown base: {other}"),
                    };
                    self.report_build_step(format!(
                        "Copy to {} from {}{}",
                        step.target, step.source, ignore_extra
                    ));
                    let source = pythonic_join(base, &step.source);
                    let target = pythonic_join(&build_path, &step.target);
                    if normalize_absolute(&source)? == normalize_absolute(&target)? {
                        return Ok(());
                    }
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    if source.is_dir() {
                        copy_tree_with_ignore(&source, &target, &ignore_matches)?;
                    } else if source.is_file() {
                        std::fs::copy(&source, &target)
                            .with_context(|| format!("Failed to copy {}", source.display()))?;
                    } else {
                        bail!("Source {} is not a file or directory", step.source);
                    }
                }
            }
            Step::Env(step) => {
                self.report_build_step(format!(
                    "Setting environment variables: {}",
                    display_environment_variables(&step.variables)
                ));
                for (key, value) in &step.variables {
                    env.insert(key.clone(), value.clone());
                }
            }
            Step::Path(step) => {
                self.report_build_step(format!("Add {} to PATH", step.path));
                let old = env.get("PATH").cloned().unwrap_or_default();
                env.insert("PATH".to_owned(), format!("{}:{}", step.path, old));
            }
            Step::WriteFile(step) => {
                let mut target = PathBuf::from(&step.path);
                if !target.is_absolute() {
                    target = build_path.join(&target);
                }
                self.report_build_step(format!("Write file {}", target.display()));
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&target, &step.content)?;
            }
        }
        Ok(())
    }
}

impl BuildBackend for LocalBuildBackend {
    fn build(
        &mut self,
        _name: &str,
        env: &IndexMap<String, String>,
        mounts: &[Mount],
        steps: &[Step],
    ) -> Result<()> {
        build_started(&self.operation);
        let started_at = std::time::Instant::now();
        let base_path = self.local_path.clone();
        let _ = std::fs::remove_dir_all(&base_path);
        std::fs::create_dir_all(&base_path)?;
        std::fs::create_dir_all(&self.build_path)?;
        for mount in mounts {
            std::fs::create_dir_all(&mount.build_path)?;
        }
        let mut env = env.clone();
        for step in steps {
            self.execute_step(step, &mut env)?;
        }

        if let Some(path) = env.get("PATH") {
            std::fs::write(base_path.join(".path"), path)?;
        }
        self.runtime_path = env.get("PATH").cloned();

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
        self.get_mount_path(name)
    }

    fn get_artifact_mount_path(&self, name: &str) -> PathBuf {
        self.get_mount_path(name)
    }

    fn get_volume_path(&self, name: &str) -> PathBuf {
        self.anybuild_dir.join("volumes").join(name)
    }

    fn get_runtime_path(&self) -> Option<String> {
        self.runtime_path.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn copy_tree_skips_ignored_basenames_at_every_level() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");
        write(&src.join("index.html"), "hello");
        write(&src.join("debug.log"), "log");
        write(&src.join("nested/keep.txt"), "keep");
        write(&src.join("nested/trace.log"), "log");
        write(&src.join("nested/node_modules/pkg/main.js"), "js");
        write(&src.join(".anybuild/state"), "state");
        write(&src.join("Anybuild"), "plan");

        copy_tree_with_ignore(
            &src,
            &dst,
            &[
                "*.log".to_owned(),
                "node_modules".to_owned(),
                ".anybuild".to_owned(),
                "Anybuild".to_owned(),
            ],
        )
        .unwrap();

        assert!(dst.join("index.html").is_file());
        assert!(dst.join("nested/keep.txt").is_file());
        assert!(!dst.join("debug.log").exists());
        assert!(!dst.join("nested/trace.log").exists());
        assert!(!dst.join("nested/node_modules").exists());
        assert!(!dst.join(".anybuild").exists());
        assert!(!dst.join("Anybuild").exists());
    }

    #[test]
    fn copy_tree_merges_into_existing_target() {
        // shutil.copytree(dirs_exist_ok=True): existing files overwritten,
        // unrelated files left alone.
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");
        write(&src.join("a.txt"), "new");
        write(&dst.join("a.txt"), "old");
        write(&dst.join("other.txt"), "keep");

        copy_tree(&src, &dst).unwrap();

        assert_eq!(std::fs::read_to_string(dst.join("a.txt")).unwrap(), "new");
        assert_eq!(
            std::fs::read_to_string(dst.join("other.txt")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn ignore_patterns_use_fnmatch_star_and_question() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");
        write(&src.join("cache-1"), "x");
        write(&src.join("cache-22"), "x");
        write(&src.join("cachet"), "x");

        copy_tree_with_ignore(&src, &dst, &["cache-?".to_owned()]).unwrap();

        assert!(!dst.join("cache-1").exists());
        assert!(dst.join("cache-22").is_file());
        assert!(dst.join("cachet").is_file());
    }

    #[test]
    fn pythonic_join_matches_pathlib() {
        assert_eq!(pythonic_join(Path::new("/a/b"), ""), Path::new("/a/b"));
        assert_eq!(pythonic_join(Path::new("/a/b"), "c"), Path::new("/a/b/c"));
        assert_eq!(
            pythonic_join(Path::new("/a/b"), "/opt/x"),
            Path::new("/opt/x")
        );
    }

    #[test]
    fn environment_variables_are_displayed_as_assignments() {
        let mut map = IndexMap::new();
        map.insert("NODE_ENV".to_owned(), "production".to_owned());
        map.insert("MESSAGE".to_owned(), "say \"hello\"\nnow".to_owned());
        assert_eq!(
            display_environment_variables(&map),
            r#"NODE_ENV="production" MESSAGE="say \"hello\"\nnow""#
        );
    }

    #[test]
    fn local_build_progress_uses_virtual_absolute_paths() {
        let root = Path::new("/Users/example/project/.anybuild/local/build");

        assert_eq!(
            shorten_local_build_paths(
                "$ mkdir -p /Users/example/project/.anybuild/local/build/opt/assets",
                root,
            ),
            "$ mkdir -p /opt/assets"
        );
        assert_eq!(
            shorten_local_build_paths(
                "Copy to /Users/example/project/.anybuild/local/build/opt/assets/optimize-node-modules.sh from node/optimize-node-modules.sh",
                root,
            ),
            "Copy to /opt/assets/optimize-node-modules.sh from node/optimize-node-modules.sh"
        );
        assert_eq!(
            shorten_local_build_paths(
                "$ bash /Users/example/project/.anybuild/local/build/opt/assets/optimize-node-modules.sh node_modules",
                root,
            ),
            "$ bash /opt/assets/optimize-node-modules.sh node_modules"
        );
        assert_eq!(
            shorten_local_build_paths(
                "$ cp -R . /Users/example/project/.anybuild/local/build/app",
                root,
            ),
            "$ cp -R . /app"
        );
        assert_eq!(
            shorten_local_build_paths("$ cd /Users/example/project/.anybuild/local/build", root,),
            "$ cd /"
        );
        assert_eq!(
            shorten_local_build_paths(
                "$ touch /Users/example/project/.anybuild/local/build-cache",
                root,
            ),
            "$ touch /Users/example/project/.anybuild/local/build-cache"
        );
    }
}
