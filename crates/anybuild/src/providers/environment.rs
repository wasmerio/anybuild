//! Environment overrides expressed as candidate JSON values.
//!
//! Serialized `Option<T>` values lose their type when they are `null`. For
//! those fields, an environment value can have more than one reasonable JSON
//! representation. This module only builds those representations; the
//! provider boundary remains responsible for deserializing them.

use anyhow::{anyhow, Result};
use serde_json::{Map, Value};

use crate::operation::OperationContext;

pub(super) struct FieldOverride {
    pub field: String,
    pub candidates: Vec<Value>,
}

pub(super) struct EnvironmentOverlay {
    pub fields: Map<String, Value>,
    pub overrides: Vec<FieldOverride>,
    pub applied: Option<(String, String)>,
}

pub(super) fn apply(json: Value, operation: &OperationContext) -> Result<EnvironmentOverlay> {
    let mut fields = json
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow!("provider config did not serialize to an object"))?;
    let mut overrides = Vec::new();
    let mut applied = None;

    for (field, current) in &mut fields {
        let Some((name, raw)) = environment_value(operation, field) else {
            continue;
        };
        let candidates = if current.is_null() {
            untyped_candidates(&raw)
        } else {
            vec![parse_typed_value(&name, &raw, current)?]
        };
        overrides.push(FieldOverride {
            field: field.clone(),
            candidates,
        });
        applied = Some((name, raw));
    }

    Ok(EnvironmentOverlay {
        fields,
        overrides,
        applied,
    })
}

fn environment_value(operation: &OperationContext, field: &str) -> Option<(String, String)> {
    let upper = field.to_ascii_uppercase();
    let anybuild = format!("ANYBUILD_{upper}");
    let shipit = format!("SHIPIT_{upper}");
    operation
        .environment_var(&anybuild)
        .map(|value| (anybuild, value))
        .or_else(|| {
            operation
                .environment_var(&shipit)
                .map(|value| (shipit, value))
        })
        .or_else(|| {
            (field == "port")
                .then(|| {
                    operation
                        .environment_var("PORT")
                        .map(|value| ("PORT".into(), value))
                })
                .flatten()
        })
}

fn parse_typed_value(name: &str, raw: &str, current: &Value) -> Result<Value> {
    let invalid =
        |expected: &str| anyhow!("Invalid value for {name}: {raw:?}; expected {expected}");
    match current {
        Value::Bool(_) => parse_bool(raw)
            .map(Value::Bool)
            .ok_or_else(|| invalid("a boolean")),
        Value::Number(number) if number.is_i64() || number.is_u64() => raw
            .trim()
            .parse::<i64>()
            .map(|value| Value::Number(value.into()))
            .map_err(|_| invalid("an integer")),
        Value::Number(_) => {
            let value = raw.trim().parse::<f64>().map_err(|_| invalid("a number"))?;
            serde_json::Number::from_f64(value)
                .map(Value::Number)
                .ok_or_else(|| invalid("a finite number"))
        }
        Value::Array(_) | Value::Object(_) => {
            serde_json::from_str(raw).map_err(|_| invalid("valid JSON"))
        }
        Value::String(_) => Ok(Value::String(raw.to_owned())),
        Value::Null => unreachable!("null values have no type to parse against"),
    }
}

/// Return plausible JSON readings in preference order. The provider's normal
/// deserializer decides which reading matches the field's erased `Option<T>`.
fn untyped_candidates(raw: &str) -> Vec<Value> {
    let mut candidates = Vec::new();
    if let Ok(parsed) = serde_json::from_str::<Value>(raw) {
        candidates.push(parsed);
    }
    if let Some(boolean) = parse_bool(raw) {
        push_unique(&mut candidates, Value::Bool(boolean));
    }
    push_unique(&mut candidates, Value::String(raw.to_owned()));
    candidates
}

fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "t" | "yes" | "y" | "on" | "1" => Some(true),
        "false" | "f" | "no" | "n" | "off" | "0" => Some(false),
        _ => None,
    }
}

fn push_unique(values: &mut Vec<Value>, value: Value) {
    if !values.contains(&value) {
        values.push(value);
    }
}
