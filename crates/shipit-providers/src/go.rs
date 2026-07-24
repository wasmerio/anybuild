//! Port of `src/shipit/providers/go.py`.
//!
//! Go module detection and build-file discovery (main.go/server.go/… at
//! the root, under src/, or one/two directory levels down).

use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::base::{env_str, is_blank, BaseConfig, DetectResult, HasBase};

pub const NAME: &str = "go";

const BUILD_FILE_CANDIDATES: [&str; 5] = ["main.go", "server.go", "serve.go", "api.go", "web.go"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoConfig {
    #[serde(flatten)]
    pub base: BaseConfig,
    pub go_version: Option<String>,
    pub go_build_file: Option<String>,
    pub serve_binary: Option<String>,
}

impl Default for GoConfig {
    fn default() -> Self {
        Self {
            base: BaseConfig::default(),
            go_version: Some("1.25.5".to_owned()),
            go_build_file: None,
            serve_binary: None,
        }
    }
}

impl HasBase for GoConfig {
    fn base(&self) -> &BaseConfig {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseConfig {
        &mut self.base
    }
}

/// Port of `GoProvider.load_config`.
///
/// Python constructs `GoConfig()` from scratch (the passed base config is
/// ignored), so every field — including the base — comes from the env or
/// its declared default.
pub fn load_config(path: &Path, _base: BaseConfig) -> Result<GoConfig> {
    let mut config = GoConfig {
        base: BaseConfig::default(),
        go_version: env_str("go_version").or_else(|| Some("1.25.5".to_owned())),
        go_build_file: env_str("go_build_file"),
        serve_binary: env_str("serve_binary"),
    };
    // Python: `if not config.go_build_file:` twice — "" gates discovery and
    // the missing-file error just like None.
    if is_blank(&config.go_build_file) {
        config.go_build_file = get_build_file(path);
    }
    let build_file = config
        .go_build_file
        .clone()
        .filter(|f| !f.is_empty())
        .with_context(|| {
        format!(
            "No Go build file was found in {}. Set SHIPIT_GO_BUILD_FILE or add a supported Go entrypoint",
            path.display()
        )
    })?;
    if is_blank(&config.serve_binary) {
        let serve_binary = build_file
            .replace('/', "_")
            .to_lowercase()
            .trim_start_matches('_')
            .replace(".go", "");
        config.serve_binary = Some(serve_binary);
    }
    if config.serve_binary.as_deref().is_none_or(str::is_empty) {
        bail!("No serve binary for Go was configured");
    }
    Ok(config)
}

/// Port of `GoProvider.get_build_file`.
pub fn get_build_file(root_path: &Path) -> Option<String> {
    for name in BUILD_FILE_CANDIDATES {
        if root_path.join(name).exists() {
            return Some(name.to_owned());
        }
        if root_path.join("src").join(name).exists() {
            return Some(format!("src/{name}"));
        }
    }
    for name in BUILD_FILE_CANDIDATES {
        // Python: next(root_path.glob(f"*/{name}")) then f"*/*/{name}".
        if let Some(found) =
            glob_child(root_path, name, 1).or_else(|| glob_child(root_path, name, 2))
        {
            return Some(found);
        }
    }
    None
}

/// `*/name` (depth 1) or `*/*/name` (depth 2), first match with sorted
/// directory entries for determinism.
fn glob_child(root: &Path, name: &str, depth: usize) -> Option<String> {
    let entries = sorted_entries(root);
    if depth == 1 {
        for entry in entries {
            if root.join(&entry).join(name).exists() {
                return Some(format!("{entry}/{name}"));
            }
        }
        return None;
    }
    for first in entries {
        let first_path = root.join(&first);
        for second in sorted_entries(&first_path) {
            if first_path.join(&second).join(name).exists() {
                return Some(format!("{first}/{second}/{name}"));
            }
        }
    }
    None
}

fn sorted_entries(path: &Path) -> Vec<String> {
    let Ok(read_dir) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut entries: Vec<String> = read_dir
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    entries.sort();
    entries
}

/// Port of `GoProvider.detect`.
pub fn detect(path: &Path, _base: &BaseConfig) -> Option<DetectResult> {
    if path.join("go.mod").exists() || path.join("go.sum").exists() {
        return Some(DetectResult {
            name: NAME,
            score: 80,
        });
    }
    None
}
