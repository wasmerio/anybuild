//! Port of `src/shipit/providers/jekyll.py`.
//!
//! Jekyll detection (_config.yml/_config.yaml, Gemfile) and destination
//! resolution.

use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::base::{env_str, BaseConfig, DetectResult, HasBase};
use crate::staticfile::{self, compute_redirects_config, parse_simple_yaml, StaticFileConfig};

pub const NAME: &str = "jekyll";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JekyllConfig {
    #[serde(flatten)]
    pub staticfile: StaticFileConfig,
    pub ruby_version: Option<String>,
    pub jekyll_version: Option<String>,
}

impl Default for JekyllConfig {
    fn default() -> Self {
        Self {
            staticfile: StaticFileConfig {
                // JekyllConfig redeclares static_dir with a "_site" default.
                static_dir: Some("_site".to_owned()),
                ..StaticFileConfig::default()
            },
            ruby_version: Some("3.4.7".to_owned()),
            jekyll_version: Some("4.3.0".to_owned()),
        }
    }
}

impl HasBase for JekyllConfig {
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

/// Port of `JekyllProvider.load_config`.
pub fn load_config(path: &Path, base: BaseConfig) -> Result<JekyllConfig> {
    let staticfile_config = staticfile::load_config(path, base)?;
    // Python re-instantiates JekyllConfig(**config.model_dump()): every
    // staticfile field is an init kwarg, so the env only reaches the
    // ruby/jekyll versions.
    let mut config = JekyllConfig {
        staticfile: staticfile_config,
        ruby_version: env_str("ruby_version").or_else(|| Some("3.4.7".to_owned())),
        jekyll_version: env_str("jekyll_version").or_else(|| Some("4.3.0".to_owned())),
    };
    if config
        .staticfile
        .static_dir
        .as_deref()
        .is_none_or(|dir| dir.is_empty())
    {
        let mut destination = None;
        for name in ["_config.yml", "_config.yaml"] {
            if path.join(name).exists() {
                if let Ok(contents) = std::fs::read_to_string(path.join(name)) {
                    destination = parse_simple_yaml(&contents).get("destination").cloned();
                }
                break;
            }
        }
        config.staticfile.static_dir = Some(destination.unwrap_or_else(|| "_site".to_owned()));
    }
    // static_dir may have changed since the base load; recompute redirects.
    config.staticfile.redirects_config = compute_redirects_config(
        path,
        config.staticfile.static_dir.as_deref(),
        config.staticfile.convert_redirects,
    )?;
    Ok(config)
}

/// Port of `JekyllProvider.detect`.
pub fn detect(path: &Path, base: &BaseConfig) -> Option<DetectResult> {
    if exists(path, &["_config.yml", "_config.yaml"]) {
        if exists(path, &["Gemfile"]) {
            return Some(DetectResult {
                name: NAME,
                score: 85,
            });
        }
        return Some(DetectResult {
            name: NAME,
            score: 40,
        });
    }
    if base
        .commands
        .build
        .as_deref()
        .is_some_and(|build| build.starts_with("jekyll "))
    {
        return Some(DetectResult {
            name: NAME,
            score: 85,
        });
    }
    None
}
