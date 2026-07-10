//! Base config, detection result, and the SHIPIT_* env overlay.
//!
//! Port of `providers/base.py`. Serialization must match pydantic's
//! `model_dump(mode="json")`: every field present, `None` as null, enums
//! as strings, sets as sorted lists (use `BTreeSet`).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::procfile::Procfile;

#[derive(Debug, Clone, PartialEq)]
pub struct DetectResult {
    pub name: &'static str,
    /// Higher score wins when multiple providers match.
    pub score: i32,
}

thread_local! {
    static ENV_DISABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Run `f` with the SHIPIT_* env overlay disabled. Used to construct the
/// *declared* field defaults (pydantic's `exclude_defaults` compares
/// against declared defaults, not env-provided values).
pub fn without_env<R>(f: impl FnOnce() -> R) -> R {
    ENV_DISABLED.with(|flag| flag.set(true));
    let result = f();
    ENV_DISABLED.with(|flag| flag.set(false));
    result
}

/// `SHIPIT_<FIELD>` environment lookup.
pub fn env_var(field: &str) -> Option<String> {
    if ENV_DISABLED.with(|flag| flag.get()) {
        return None;
    }
    std::env::var(format!("SHIPIT_{}", field.to_uppercase())).ok()
}

pub fn env_str(field: &str) -> Option<String> {
    env_var(field).filter(|s| !s.is_empty())
}

/// pydantic v2 bool coercion from strings.
pub fn env_bool(field: &str) -> Option<bool> {
    let raw = env_var(field)?;
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "t" | "yes" | "y" | "on" | "1" => Some(true),
        "false" | "f" | "no" | "n" | "off" | "0" => Some(false),
        _ => None,
    }
}

pub fn env_int(field: &str) -> Option<i64> {
    env_var(field)?.trim().parse().ok()
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CustomCommands {
    pub install: Option<String>,
    pub build: Option<String>,
    pub start: Option<String>,
    pub after_deploy: Option<String>,
}

impl CustomCommands {
    /// Port of `CustomCommands.enrich_from_path`: pick up a Procfile's
    /// start command when present.
    pub fn enrich_from_path(&mut self, path: &Path) {
        let procfile_path = path.join("Procfile");
        if let Ok(contents) = std::fs::read_to_string(&procfile_path) {
            if let Ok(procfile) = Procfile::loads(&contents) {
                if let Some(start) = procfile.get_start_command() {
                    self.start = Some(start.to_owned());
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaseConfig {
    /// Serve name; defaults to the project directory name (set by the CLI).
    pub name: Option<String>,
    pub port: Option<i64>,
    pub commands: CustomCommands,
    /// Subdirectory of the workspace the app lives in (set by the CLI
    /// after load; recorded in the generated Shipit file).
    pub app_subdir: Option<String>,
}

impl Default for BaseConfig {
    fn default() -> Self {
        Self {
            name: env_str("name"),
            port: env_int("port").or(Some(8080)),
            commands: CustomCommands::default(),
            app_subdir: env_str("app_subdir"),
        }
    }
}

/// Every provider config embeds the base and exposes it uniformly.
pub trait HasBase {
    fn base(&self) -> &BaseConfig;
    fn base_mut(&mut self) -> &mut BaseConfig;
}
