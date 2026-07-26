//! Base config and the ANYBUILD_*/SHIPIT_* environment overlay.
//!
//! Port of `providers/base.py`. Serialization must match pydantic's
//! `model_dump(mode="json")`: every field present, `None` as null, enums
//! as strings, sets as sorted lists (use `BTreeSet`).

use std::path::Path;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::operation::OperationContext;
use crate::providers::procfile::Procfile;

fn env_value(operation: &OperationContext, field: &str) -> Option<(String, String)> {
    let field = field.to_uppercase();
    for prefix in ["ANYBUILD_", "SHIPIT_"] {
        let name = format!("{prefix}{field}");
        if let Some(value) = operation.environment_var(&name) {
            return Some((name, value));
        }
    }
    None
}

/// `ANYBUILD_<FIELD>` environment lookup, falling back to the legacy
/// `SHIPIT_<FIELD>` name only when the Anybuild variable is absent.
pub fn env_var(operation: &OperationContext, field: &str) -> Option<String> {
    env_value(operation, field).map(|(_, value)| value)
}

/// String settings preserve an explicitly empty value, matching
/// pydantic-settings rather than treating it as absent.
pub fn env_str(operation: &OperationContext, field: &str) -> Option<String> {
    env_var(operation, field)
}

/// Python truthiness for optional strings: `None` and `""` are both falsy.
/// Use wherever the Python original tests `if not config.field:`, so an
/// explicitly empty `ANYBUILD_*` value still triggers the fallback.
pub fn is_blank(value: &Option<String>) -> bool {
    value.as_deref().is_none_or(str::is_empty)
}

/// pydantic v2 bool coercion from strings.
pub fn env_bool(operation: &OperationContext, field: &str) -> Result<Option<bool>> {
    let Some((name, raw)) = env_value(operation, field) else {
        return Ok(None);
    };
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "t" | "yes" | "y" | "on" | "1" => Ok(Some(true)),
        "false" | "f" | "no" | "n" | "off" | "0" => Ok(Some(false)),
        _ => Err(invalid_env_value(
            &name,
            &raw,
            "a boolean (true/false, yes/no, on/off, or 1/0)",
        )),
    }
}

pub fn env_int(operation: &OperationContext, field: &str) -> Result<Option<i64>> {
    let Some((name, raw)) = env_value(operation, field) else {
        return Ok(None);
    };
    raw.trim()
        .parse()
        .map(Some)
        .map_err(|_| invalid_env_value(&name, &raw, "an integer"))
}

pub fn env_enum<T>(
    operation: &OperationContext,
    field: &str,
    expected: &str,
    parse: impl FnOnce(&str) -> Option<T>,
) -> Result<Option<T>> {
    let Some((name, raw)) = env_value(operation, field) else {
        return Ok(None);
    };
    parse(&raw)
        .map(Some)
        .ok_or_else(|| invalid_env_value(&name, &raw, expected))
}

pub fn env_json<T: serde::de::DeserializeOwned>(
    operation: &OperationContext,
    field: &str,
) -> Result<Option<T>> {
    let Some((name, raw)) = env_value(operation, field) else {
        return Ok(None);
    };
    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|err| anyhow!("Invalid value for {name}: {raw:?}; expected valid JSON: {err}"))
}

fn invalid_env_value(name: &str, raw: &str, expected: &str) -> anyhow::Error {
    anyhow!("Invalid value for {name}: {raw:?}; expected {expected}")
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
    /// after load; recorded in the generated Anybuild file).
    pub app_subdir: Option<String>,
}

impl Default for BaseConfig {
    fn default() -> Self {
        Self {
            name: None,
            port: Some(8080),
            commands: CustomCommands::default(),
            app_subdir: None,
        }
    }
}

impl BaseConfig {
    /// Apply the fallible Anybuild overlay to the declared defaults.
    pub fn from_env(operation: &OperationContext) -> Result<Self> {
        Ok(Self {
            name: env_str(operation, "name"),
            port: env_int(operation, "port")?.or(Some(8080)),
            commands: CustomCommands::default(),
            app_subdir: env_str(operation, "app_subdir"),
        })
    }
}

/// Every provider config embeds the base and exposes it uniformly.
pub trait HasBase {
    fn base(&self) -> &BaseConfig;
    fn base_mut(&mut self) -> &mut BaseConfig;
}
