//! Starlark config value for Shipit files.
//!
//! This module provides the `ShipitConfig` type that exposes
//! provider-detected configuration values to Starlark code via
//! attribute access (e.g., `config.php_version`).

use allocative::Allocative;
use starlark::starlark_simple_value;
use starlark::values::starlark_value;
use starlark::values::{Heap, NoSerialize, ProvidesStaticType, StarlarkValue, Value};
use std::collections::HashMap;
use std::fmt;

/// A configuration value that can be stored in `ShipitConfig`.
#[derive(Debug, Clone, PartialEq, Allocative)]
pub enum ConfigValue {
    /// A string value (e.g., "8.3", "64-bit").
    String(String),
    /// A boolean value (e.g., `precompile_python`).
    Bool(bool),
    /// An absent/null value — returned as `None` in Starlark.
    None,
}

impl ConfigValue {
    /// Create a string config value. Returns `ConfigValue::None` if
    /// the input is `None`.
    pub fn from_option(opt: Option<String>) -> Self {
        match opt {
            Some(s) => ConfigValue::String(s),
            None => ConfigValue::None,
        }
    }
}

/// Starlark value exposing provider config fields via attribute
/// access. Every Shipit file receives a global `config` variable of
/// this type.
#[derive(Debug, Clone, ProvidesStaticType, NoSerialize, Allocative)]
pub struct ShipitConfig {
    /// Arbitrary key-value pairs populated by the provider.
    pub fields: HashMap<String, ConfigValue>,
}

starlark_simple_value!(ShipitConfig);

impl fmt::Display for ShipitConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ShipitConfig({{")?;
        let mut first = true;
        // Sort keys for deterministic output
        let mut keys: Vec<_> = self.fields.keys().collect();
        keys.sort();
        for key in keys {
            if !first {
                write!(f, ", ")?;
            }
            first = false;
            match &self.fields[key] {
                ConfigValue::String(s) => write!(f, "{}: \"{}\"", key, s)?,
                ConfigValue::Bool(b) => write!(f, "{}: {}", key, b)?,
                ConfigValue::None => write!(f, "{}: None", key)?,
            }
        }
        write!(f, "}})")
    }
}

#[starlark_value(type = "ShipitConfig")]
impl<'v> StarlarkValue<'v> for ShipitConfig {
    fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match self.fields.get(attribute) {
            Some(ConfigValue::String(s)) => Some(heap.alloc(s.as_str())),
            Some(ConfigValue::Bool(b)) => Some(heap.alloc(*b)),
            Some(ConfigValue::None) | None => Some(Value::new_none()),
        }
    }

    fn has_attr(&self, attribute: &str, _heap: &'v Heap) -> bool {
        self.fields.contains_key(attribute)
    }

    fn dir_attr(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.fields.keys().cloned().collect();
        keys.sort();
        keys
    }
}

impl ShipitConfig {
    /// Create an empty config.
    pub fn new() -> Self {
        Self {
            fields: HashMap::new(),
        }
    }

    /// Set a string field.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.fields
            .insert(key.into(), ConfigValue::String(value.into()));
    }

    /// Set a boolean field.
    pub fn set_bool(&mut self, key: impl Into<String>, value: bool) {
        self.fields.insert(key.into(), ConfigValue::Bool(value));
    }

    /// Set an optional string field. Stores `ConfigValue::None` when
    /// the input is `None`.
    pub fn set_option(&mut self, key: impl Into<String>, value: Option<impl Into<String>>) {
        self.fields.insert(
            key.into(),
            match value {
                Some(v) => ConfigValue::String(v.into()),
                None => ConfigValue::None,
            },
        );
    }
}

impl Default for ShipitConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_set_and_display() {
        let mut config = ShipitConfig::new();
        config.set("php_version", "8.3");
        config.set_bool("precompile_python", true);
        config.set_option("cross_platform", None::<String>);

        let display = config.to_string();
        assert!(display.contains("php_version: \"8.3\""));
        assert!(display.contains("precompile_python: true"));
        assert!(display.contains("cross_platform: None"));
    }
}
