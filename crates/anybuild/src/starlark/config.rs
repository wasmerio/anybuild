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

#[derive(Debug, Clone, ProvidesStaticType, NoSerialize, Allocative)]
pub struct ConfigValue(#[allocative(skip)] pub serde_json::Map<String, Json>);

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
        Json::Object(map) => heap.alloc(ConfigValue(map.clone())),
    }
}

#[starlark_value(type = "config")]
impl<'v> StarlarkValue<'v> for ConfigValue {
    fn get_attr(&self, attribute: &str, heap: Heap<'v>) -> Option<Value<'v>> {
        self.0.get(attribute).map(|json| json_to_value(heap, json))
    }

    fn dir_attr(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }

    /// Allow dict-style reads too (`config["x"]`), cheap and harmless.
    fn at(&self, index: Value<'v>, heap: Heap<'v>) -> starlark::Result<Value<'v>> {
        let key = index.unpack_str().ok_or_else(|| {
            starlark::Error::new_other(anyhow::anyhow!("config keys are strings"))
        })?;
        match self.0.get(key) {
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
