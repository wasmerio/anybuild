//! MkDocs provider implementation.
//!
//! `MkdocsConfig(PythonConfig, StaticFileConfig)`: the python config
//! (loaded with `must_have_deps={"mkdocs"}`) merged with the staticfile
//! config and the base config, exactly as Python's
//! `python_dump | staticfile_dump | base_dump`. The merge happens on the
//! JSON views so this port stays decoupled from the python provider's
//! struct layout.

use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::operation::OperationContext;
use crate::providers::base::{env_str, BaseConfig, HasBase};
use crate::providers::python::PythonConfig;
use crate::providers::staticfile;
use crate::providers::Provider;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MkdocsConfig {
    #[serde(flatten)]
    pub python: PythonConfig,
    // StaticFileConfig fields (python's flatten already carries the base).
    pub convert_redirects: bool,
    pub sws_version: Option<String>,
    pub static_dir: Option<String>,
    pub redirects_config: Option<String>,
    pub mkdocs_version: Option<String>,
}

impl Default for MkdocsConfig {
    fn default() -> Self {
        Self {
            python: default_python_config(),
            convert_redirects: true,
            sws_version: Some("2.38.0".to_owned()),
            static_dir: None,
            redirects_config: None,
            mkdocs_version: None,
        }
    }
}

/// A `PythonConfig` with its declared pydantic defaults, built through the
/// JSON view so it tracks the sibling python port's field set without a
/// compile-time dependency on it (unknown keys are ignored while the port
/// is still a stub).
fn default_python_config() -> PythonConfig {
    let mut map = match serde_json::to_value(BaseConfig::default()) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    };
    for (key, value) in [
        ("framework", Value::Null),
        ("server", Value::Null),
        ("migration_strategy", Value::Null),
        ("database", Value::Null),
        ("extra_dependencies", Value::Array(Vec::new())),
        ("asgi_application", Value::Null),
        ("wsgi_application", Value::Null),
        ("uses_ffmpeg", Value::Bool(false)),
        ("uses_pandoc", Value::Bool(false)),
        ("install_requires_all_files", Value::Bool(false)),
        ("main_file", Value::Null),
        ("python_version", Value::String("3.13".to_owned())),
        ("uv_version", Value::String("0.8.15".to_owned())),
        ("precompile_python", Value::Bool(true)),
        ("cross_platform", Value::Null),
        ("python_extra_index_url", Value::Null),
        ("pandoc_version", Value::Null),
        ("ffmpeg_version", Value::Null),
        ("install_inputs", Value::Null),
        ("mcp_self_running", Value::Bool(false)),
    ] {
        map.insert(key.to_owned(), value);
    }
    serde_json::from_value(Value::Object(map)).expect("python config defaults deserialize")
}

impl HasBase for MkdocsConfig {
    fn base(&self) -> &BaseConfig {
        self.python.base()
    }
    fn base_mut(&mut self) -> &mut BaseConfig {
        self.python.base_mut()
    }
}

impl MkdocsConfig {
    /// Mkdocs inherits python's `cross_platform` field.
    #[cfg(test)]
    pub fn set_cross_platform(&mut self, value: &str) -> bool {
        self.python.cross_platform = Some(value.to_owned());
        true
    }
}

fn exists(path: &Path, candidates: &[&str]) -> bool {
    candidates.iter().any(|c| path.join(c).exists())
}

fn to_object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        other => panic!("config serializes to an object, got {other}"),
    }
}

/// Whether "mkdocs" appears in the project's Python dependency files —
/// the `must_have_deps={"mkdocs"}` half of
/// `PythonProvider.load_config(path, base, must_have_deps={"mkdocs"})`.
/// Mirrors `PythonProvider.check_deps` (substring match per line over the
/// discovered dependency files, skipping uv.lock).
fn python_deps_contain(path: &Path, dep: &str) -> bool {
    let mut files = Vec::new();
    if path.join("pyproject.toml").is_file() {
        files.push(path.join("pyproject.toml"));
    }
    if path.join("requirements.txt").is_file() {
        collect_requirements_files(path.join("requirements.txt"), &mut files);
    }
    let needle = dep.to_lowercase();
    for file in files {
        let Ok(contents) = std::fs::read_to_string(&file) else {
            continue;
        };
        if contents
            .lines()
            .any(|line| line.to_lowercase().contains(&needle))
        {
            return true;
        }
    }
    false
}

/// requirements.txt plus `-r`/`-c` includes (a light version of
/// `discover_python_dependency_files`).
fn collect_requirements_files(file: std::path::PathBuf, out: &mut Vec<std::path::PathBuf>) {
    if out.contains(&file) || !file.is_file() {
        return;
    }
    let Ok(contents) = std::fs::read_to_string(&file) else {
        return;
    };
    let parent = file.parent().map(|p| p.to_path_buf());
    out.push(file);
    let Some(parent) = parent else {
        return;
    };
    for line in contents.lines() {
        let trimmed = line.trim();
        for prefix in ["-r ", "--requirement ", "-c ", "--constraint "] {
            if let Some(reference) = trimmed.strip_prefix(prefix) {
                collect_requirements_files(parent.join(reference.trim()), out);
            }
        }
        for prefix in ["-r", "--requirement=", "-c", "--constraint="] {
            if let Some(reference) = trimmed.strip_prefix(prefix) {
                if !reference.is_empty() && !reference.starts_with([' ', '=']) {
                    collect_requirements_files(parent.join(reference.trim()), out);
                }
            }
        }
    }
}

/// Port of `MkdocsProvider.load_config`.
pub fn load_config(
    path: &Path,
    base: BaseConfig,
    operation: &OperationContext,
) -> Result<MkdocsConfig> {
    let python_config = crate::providers::python::load_config(path, base.clone(), operation)?;
    let staticfile_config = staticfile::load_config(path, base.clone(), operation)?;

    let mut merged =
        to_object(serde_json::to_value(&python_config).expect("python config serializes"));

    // must_have_deps={"mkdocs"}: python's load_config adds any must-have
    // dep that is missing from the dependency files to extra_dependencies.
    if !python_deps_contain(path, "mkdocs") {
        let deps = merged
            .entry("extra_dependencies".to_owned())
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Value::Array(items) = deps {
            let dep = Value::String("mkdocs".to_owned());
            if !items.contains(&dep) {
                items.push(dep);
                items.sort_by(|a, b| a.as_str().unwrap_or("").cmp(b.as_str().unwrap_or("")));
            }
        }
    }

    merged.extend(to_object(
        serde_json::to_value(&staticfile_config).expect("staticfile config serializes"),
    ));
    merged.extend(to_object(
        serde_json::to_value(&base).expect("base config serializes"),
    ));
    // mkdocs_version is the only field that is not an init kwarg in
    // Python, so it alone sees the env.
    merged.insert(
        "mkdocs_version".to_owned(),
        env_str(operation, "mkdocs_version").map_or(Value::Null, Value::String),
    );

    Ok(serde_json::from_value(Value::Object(merged))?)
}

impl Provider for MkdocsConfig {
    type Evidence = ();

    const NAME: &'static str = "mkdocs";
    const DETECTION_DETAILS: &'static [(&'static str, &'static str)] = &[
        ("MkDocs version", "mkdocs_version"),
        ("Python version", "python_version"),
        ("Output directory", "static_dir"),
    ];

    fn detection_evidence(
        path: &Path,
        base: &BaseConfig,
        _operation: &OperationContext,
    ) -> Option<Self::Evidence> {
        (exists(path, &["mkdocs.yml", "mkdocs.yaml"])
            || base
                .commands
                .build
                .as_deref()
                .is_some_and(|build| build.starts_with("mkdocs ")))
        .then_some(())
    }

    fn load(path: &Path, base: BaseConfig, operation: &OperationContext) -> Result<Self> {
        load_config(path, base, operation)
    }
}
