//! The `config` value exposed to Anybuild files.
//!
//! Rust's `config_view`: an attribute-accessible, read-only view over the
//! provider config's JSON form (`model_dump(mode="json")` with sets
//! sorted). JSON objects become nested namespaces, arrays become lists,
//! and unknown attributes raise — honest access, same as Python.

use std::fmt;

use allocative::Allocative;
use serde_json::Value as Json;
use starlark::any::ProvidesStaticType;
use starlark::values::dict::AllocDict;
use starlark::values::list::AllocList;
use starlark::values::starlark_value;
use starlark::values::{Heap, NoSerialize, StarlarkValue, Value};

use crate::internal::paths::ProjectPaths;
use crate::operation::OperationContext;
use crate::providers::{
    apply_environment, finalize_config, load_explicit_provider, workspace, BaseConfig,
    ProviderConfig,
};
use crate::sdk::CommandOverrides;

#[derive(Debug, Clone, ProvidesStaticType, NoSerialize, Allocative)]
pub struct ConfigValue {
    #[allocative(skip)]
    pub values: serde_json::Map<String, Json>,
    pub provider: Option<String>,
}

impl ConfigValue {
    pub fn nested(values: serde_json::Map<String, Json>) -> Self {
        Self {
            values,
            provider: None,
        }
    }

    pub fn provider(values: serde_json::Map<String, Json>, provider: impl Into<String>) -> Self {
        Self {
            values,
            provider: Some(provider.into()),
        }
    }
}

#[derive(Clone)]
pub struct ConfigResolutionOptions {
    pub paths: ProjectPaths,
    pub overrides: CommandOverrides,
    pub wasmer: bool,
    pub operation: OperationContext,
}

#[derive(Debug, Clone)]
pub struct PersistedConfig {
    pub provider: String,
    pub schema: u32,
    pub values: Json,
}

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub persisted: PersistedConfig,
    pub effective: ProviderConfig,
}

impl ConfigResolutionOptions {
    pub fn resolve(
        &self,
        provider: &str,
        schema: u32,
        persisted: Json,
    ) -> anyhow::Result<ResolvedConfig> {
        let expected_schema = crate::providers::provider_schema(provider)?;
        if schema != expected_schema {
            anyhow::bail!(
                "Unsupported {provider} config schema {schema}; expected {}. Run `anybuild generate` to update the Anybuild file",
                expected_schema
            );
        }
        let persisted = persisted
            .as_object()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("persisted provider config must be a dictionary"))?;

        let mut base = BaseConfig::default();
        base.commands.enrich_from_path(&self.paths.app_path);
        let mut config =
            load_explicit_provider(provider, &self.paths.app_path, &base, &self.operation)?;
        workspace::apply_subdir_provider_config(&mut config, self.paths.subdir.as_deref());
        config.apply_workspace_config(&self.paths.workspace_root);

        validate_patch(&config.to_json(), &Json::Object(persisted.clone()), "")?;
        config = config.merge_json(&Json::Object(persisted.clone()))?;
        config = apply_environment(config, &self.operation)?;
        config = apply_command_overrides(config, &self.overrides)?;
        if let Some(patch) = &self.overrides.config {
            config = config.merge_json(patch)?;
        }
        if self.wasmer {
            let patch = crate::run::wasmer::provider_config_patch(&config);
            config = config.merge_json(&patch)?;
        }
        workspace::apply_subdir_provider_config(&mut config, self.paths.subdir.as_deref());
        config = finalize_config(&self.paths.app_path, config);
        config.validate(&self.paths.app_path)?;

        if let Some(expected) = self.overrides.use_provider.as_deref() {
            if !expected.eq_ignore_ascii_case(provider) {
                anyhow::bail!(
                    "The Anybuild file declares provider {provider:?}, but {expected:?} was requested. Run `anybuild generate --provider {expected}` to regenerate it"
                );
            }
        }

        self.operation
            .provider_declared(provider, config.detection_details());
        Ok(ResolvedConfig {
            persisted: PersistedConfig {
                provider: provider.to_owned(),
                schema,
                values: Json::Object(persisted),
            },
            effective: config,
        })
    }
}

fn apply_command_overrides(
    config: ProviderConfig,
    overrides: &CommandOverrides,
) -> anyhow::Result<ProviderConfig> {
    let mut json = config.to_json();
    let object = json
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("provider config must be a dictionary"))?;
    let commands = object
        .entry("commands")
        .or_insert_with(|| serde_json::json!({}));
    let commands = commands
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("provider commands must be a dictionary"))?;
    if let Some(value) = &overrides.start_command {
        commands.insert("start".to_owned(), Json::String(value.clone()));
    }
    if let Some(value) = &overrides.install_command {
        commands.insert("install".to_owned(), Json::String(value.clone()));
    }
    if let Some(value) = &overrides.build_command {
        commands.insert("build".to_owned(), Json::String(value.clone()));
    }
    if let Some(value) = overrides.serve_port {
        object.insert("port".to_owned(), Json::Number(value.into()));
    }
    let mut updated = crate::providers::config_from_json(config.provider_name(), json)?;
    updated.copy_transient_fields_from(&config);
    Ok(updated)
}

fn validate_patch(target: &Json, patch: &Json, prefix: &str) -> anyhow::Result<()> {
    let (Some(target), Some(patch)) = (target.as_object(), patch.as_object()) else {
        return Ok(());
    };
    for (key, value) in patch {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        let Some(current) = target.get(key) else {
            anyhow::bail!("Unknown persisted config field {path:?}");
        };
        if current.is_object() && value.is_object() {
            validate_patch(current, value, &path)?;
        }
    }
    Ok(())
}

impl fmt::Display for ConfigValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "config(...)")
    }
}

pub fn json_to_value<'v>(heap: Heap<'v>, json: &Json) -> Value<'v> {
    match json {
        Json::Null => Value::new_none(),
        Json::Bool(b) => Value::new_bool(*b),
        Json::Number(n) => {
            if let Some(i) = n.as_i64() {
                heap.alloc(i)
            } else {
                heap.alloc(n.as_f64().unwrap_or(0.0))
            }
        }
        Json::String(s) => heap.alloc(s.as_str()),
        Json::Array(items) => {
            let values: Vec<Value<'v>> =
                items.iter().map(|item| json_to_value(heap, item)).collect();
            heap.alloc(AllocList(values))
        }
        Json::Object(map) => heap.alloc(ConfigValue::nested(map.clone())),
    }
}

#[starlark_value(type = "config")]
impl<'v> StarlarkValue<'v> for ConfigValue {
    fn get_attr(&self, attribute: &str, heap: Heap<'v>) -> Option<Value<'v>> {
        if attribute == "provider" {
            return self
                .provider
                .as_deref()
                .map(|provider| heap.alloc(provider));
        }
        self.values
            .get(attribute)
            .map(|json| json_to_value(heap, json))
    }

    fn dir_attr(&self) -> Vec<String> {
        let mut fields: Vec<String> = self.values.keys().cloned().collect();
        if self.provider.is_some() {
            fields.push("provider".to_owned());
        }
        fields
    }

    /// Allow dict-style reads too (`config["x"]`), cheap and harmless.
    fn at(&self, index: Value<'v>, heap: Heap<'v>) -> starlark::Result<Value<'v>> {
        let key = index.unpack_str().ok_or_else(|| {
            starlark::Error::new_other(anyhow::anyhow!("config keys are strings"))
        })?;
        match self.values.get(key) {
            Some(json) => Ok(json_to_value(heap, json)),
            None => Err(starlark::Error::new_other(anyhow::anyhow!(
                "config has no key {key:?}"
            ))),
        }
    }
}

starlark::starlark_simple_value!(ConfigValue);

/// Unused helper kept close to the bridge: dict allocation for maps when a
/// plain dict (not a namespace) is ever needed.
#[allow(dead_code)]
pub fn json_object_to_dict<'v>(heap: Heap<'v>, map: &serde_json::Map<String, Json>) -> Value<'v> {
    let entries: Vec<(Value<'v>, Value<'v>)> = map
        .iter()
        .map(|(k, v)| (heap.alloc(k.as_str()), json_to_value(heap, v)))
        .collect();
    heap.alloc(AllocDict(entries))
}
