//! Hugo provider implementation.
//!
//! Hugo site detection, publishDir resolution from hugo.{toml,json,yaml,yml}
//! and the old-Hugo (`resources.ToCSS`) version pin.

use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::operation::OperationContext;
use crate::providers::base::{env_str, is_blank, BaseConfig, HasBase};
use crate::providers::staticfile::{
    self, compute_redirects_config, parse_simple_yaml, StaticFileConfig,
};

pub const NAME: &str = "hugo";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HugoConfig {
    #[serde(flatten)]
    pub staticfile: StaticFileConfig,
    pub hugo_version: Option<String>,
}

impl Default for HugoConfig {
    fn default() -> Self {
        Self {
            staticfile: StaticFileConfig {
                // HugoConfig redeclares static_dir with a "public" default.
                static_dir: Some("public".to_owned()),
                ..StaticFileConfig::default()
            },
            hugo_version: None,
        }
    }
}

impl HasBase for HugoConfig {
    fn base(&self) -> &BaseConfig {
        self.staticfile.base()
    }
    fn base_mut(&mut self) -> &mut BaseConfig {
        self.staticfile.base_mut()
    }
}

fn exists(path: &Path, candidates: &[&str]) -> bool {
    candidates.iter().any(|c| path.join(c).exists())
}

/// Port of `_contains_text`.
fn contains_text(src_dir: &Path, text: &str) -> bool {
    let skipped_dirs = [".git", ".anybuild", "node_modules"];
    let walker = walkdir::WalkDir::new(src_dir)
        .into_iter()
        .filter_entry(|entry| {
            !skipped_dirs
                .iter()
                .any(|skipped| entry.file_name() == *skipped)
        });
    for entry in walker.filter_map(|entry| entry.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(bytes) = std::fs::read(entry.path()) else {
            continue;
        };
        if String::from_utf8_lossy(&bytes).contains(text) {
            return true;
        }
    }
    false
}

/// Port of `is_old_hugo`.
pub fn is_old_hugo(src_dir: &Path) -> bool {
    // Hard-fail signal: old Hugo. (The modern `css.Sass` probe in Python
    // returns False either way, so only this check matters.)
    contains_text(src_dir, "resources.ToCSS")
}

/// The `publishDir` (falling back to `destination`) from the hugo config
/// file, mirroring the Python toml/json/yaml loads.
fn hugo_publish_dir(path: &Path) -> Option<String> {
    if path.join("hugo.toml").exists() {
        let contents = std::fs::read_to_string(path.join("hugo.toml")).ok()?;
        let value: toml::Value = toml::from_str(&contents).ok()?;
        let table = value.as_table()?;
        return table
            .get("publishDir")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
            .or_else(|| table.get("destination").and_then(|v| v.as_str()))
            .map(|v| v.to_owned());
    }
    if path.join("hugo.json").exists() {
        let contents = std::fs::read_to_string(path.join("hugo.json")).ok()?;
        let value: serde_json::Value = serde_json::from_str(&contents).ok()?;
        return value
            .get("publishDir")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
            .or_else(|| value.get("destination").and_then(|v| v.as_str()))
            .map(|v| v.to_owned());
    }
    for name in ["hugo.yaml", "hugo.yml"] {
        if path.join(name).exists() {
            let contents = std::fs::read_to_string(path.join(name)).ok()?;
            let parsed = parse_simple_yaml(&contents);
            return parsed
                .get("publishDir")
                .or_else(|| parsed.get("destination"))
                .cloned();
        }
    }
    None
}

/// Port of `HugoProvider.load_config`.
pub fn load_config(
    path: &Path,
    base: BaseConfig,
    operation: &OperationContext,
) -> Result<HugoConfig> {
    let staticfile_config = staticfile::load_config(path, base, operation)?;
    // Python re-instantiates HugoConfig(**config.model_dump()): every
    // staticfile field is an init kwarg, so the env only reaches
    // hugo_version.
    let mut config = HugoConfig {
        staticfile: staticfile_config,
        hugo_version: env_str(operation, "hugo_version"),
    };
    if config
        .staticfile
        .static_dir
        .as_deref()
        .is_none_or(|dir| dir.is_empty())
    {
        config.staticfile.static_dir =
            Some(hugo_publish_dir(path).unwrap_or_else(|| "public".to_owned()));
    }
    // Python: `if not config.hugo_version:` — "" falls back like None.
    if is_blank(&config.hugo_version) {
        config.hugo_version = Some(if is_old_hugo(path) {
            "0.139.0".to_owned()
        } else {
            "0.153.2".to_owned()
        });
    }
    // static_dir may have changed since the base load; recompute redirects.
    config.staticfile.redirects_config = compute_redirects_config(
        path,
        config.staticfile.static_dir.as_deref(),
        config.staticfile.convert_redirects,
    )?;
    Ok(config)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DetectionEvidence {
    Strong,
    Structural,
}

pub(crate) fn detection_evidence(path: &Path, base: &BaseConfig) -> Option<DetectionEvidence> {
    if exists(path, &["hugo.toml", "hugo.json", "hugo.yaml", "hugo.yml"]) {
        return Some(DetectionEvidence::Strong);
    }
    if exists(
        path,
        &["config.toml", "config.json", "config.yaml", "config.yml"],
    ) && exists(path, &["content"])
        && (exists(path, &["static"]) || exists(path, &["themes"]))
    {
        return Some(DetectionEvidence::Structural);
    }
    if base
        .commands
        .build
        .as_deref()
        .is_some_and(|build| build.starts_with("hugo "))
    {
        return Some(DetectionEvidence::Strong);
    }
    None
}

pub fn detect_and_load(
    path: &Path,
    base: &BaseConfig,
    operation: &OperationContext,
) -> Result<Option<HugoConfig>> {
    detection_evidence(path, base)
        .map(|_| load_config(path, base.clone(), operation))
        .transpose()
}
