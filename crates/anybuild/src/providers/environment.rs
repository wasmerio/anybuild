//! Environment overrides for serialized provider configurations.

use anyhow::{anyhow, Result};

use super::{config_from_json, ProviderConfig};
use crate::operation::OperationContext;

pub(crate) fn apply_environment(
    config: ProviderConfig,
    operation: &OperationContext,
) -> Result<ProviderConfig> {
    let provider = config.provider_name();
    let mut json = config.to_json();
    let fields = json
        .as_object_mut()
        .ok_or_else(|| anyhow!("{provider} config did not serialize to an object"))?;
    let mut applied = None;
    for field in fields.keys().cloned().collect::<Vec<_>>() {
        let upper = field.to_ascii_uppercase();
        let anybuild = format!("ANYBUILD_{upper}");
        let shipit = format!("SHIPIT_{upper}");
        let Some((name, raw)) = operation
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
        else {
            continue;
        };
        let current = fields.get(&field).cloned().unwrap_or_default();
        let value = if current.is_null() {
            resolve_untyped_environment_value(&raw, |candidate| {
                let mut probe = fields.clone();
                probe.insert(field.clone(), candidate.clone());
                config_from_json(provider, serde_json::Value::Object(probe)).is_ok()
            })
        } else {
            parse_environment_value(&name, &raw, &current)?
        };
        fields.insert(field, value);
        applied = Some((name, raw));
    }
    let mut updated = config_from_json(provider, serde_json::Value::Object(fields.clone()))
        .map_err(|error| match applied {
            Some((name, raw)) => anyhow!("Invalid value for {name}: {raw:?}: {error}"),
            None => error,
        })?;
    updated.copy_transient_fields_from(&config);
    Ok(updated)
}

fn parse_environment_value(
    name: &str,
    raw: &str,
    current: &serde_json::Value,
) -> Result<serde_json::Value> {
    use serde_json::Value;
    let invalid =
        |expected: &str| anyhow!("Invalid value for {name}: {raw:?}; expected {expected}");
    match current {
        Value::Bool(_) => parse_environment_bool(raw)
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
        Value::Null => unreachable!("untyped values are resolved before parsing"),
    }
}

/// Choose a JSON representation without coupling environment parsing to a
/// particular schema or deserializer.
fn resolve_untyped_environment_value(
    raw: &str,
    accepts: impl Fn(&serde_json::Value) -> bool,
) -> serde_json::Value {
    use serde_json::Value;
    let string = Value::String(raw.to_owned());
    let json = serde_json::from_str::<Value>(raw).ok();
    let fallback = json.clone().unwrap_or_else(|| string.clone());
    json.into_iter()
        .chain(parse_environment_bool(raw).map(Value::Bool))
        .chain([string])
        .find(|candidate| accepts(candidate))
        .unwrap_or(fallback)
}

fn parse_environment_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "t" | "yes" | "y" | "on" | "1" => Some(true),
        "false" | "f" | "no" | "n" | "off" | "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use super::*;
    use crate::event::{ProcessIo, Reporter};
    use crate::providers::wordpress;

    fn apply(config: ProviderConfig, name: &str, value: &str) -> Result<serde_json::Value> {
        let environment: IndexMap<String, String> =
            [(name.to_owned(), value.to_owned())].into_iter().collect();
        // Do not let unrelated ANYBUILD_* variables in the developer's shell
        // affect these overlay tests.
        let operation =
            OperationContext::new(environment, false, ProcessIo::Inherit, Reporter::default());
        apply_environment(config, &operation).map(|config| config.to_json())
    }

    /// An `Option<bool>` left unset serializes to `null`, which carries no
    /// type. It must still accept every boolean spelling a set `bool` does,
    /// rather than reading `1` as an integer and rejecting it.
    #[test]
    fn unset_optional_bool_accepts_numeric_booleans() {
        for (raw, expected) in [
            ("1", true),
            ("0", false),
            ("true", true),
            ("false", false),
            ("yes", true),
            ("off", false),
        ] {
            let config = ProviderConfig::Wordpress(wordpress::WordPressConfig::default());
            assert_eq!(config.to_json()["phpix"], serde_json::Value::Null);
            let json = apply(config, "ANYBUILD_PHPIX", raw)
                .unwrap_or_else(|error| panic!("ANYBUILD_PHPIX={raw}: {error}"));
            assert_eq!(json["phpix"], serde_json::json!(expected), "PHPIX={raw}");
        }
    }

    /// The legacy prefix reaches the same overlay, so it needs the same
    /// coercion.
    #[test]
    fn unset_optional_bool_accepts_numeric_booleans_via_legacy_prefix() {
        let config = ProviderConfig::Wordpress(wordpress::WordPressConfig::default());
        let json = apply(config, "SHIPIT_PHPIX", "1").expect("SHIPIT_PHPIX=1 is accepted");
        assert_eq!(json["phpix"], serde_json::json!(true));
    }

    /// A value that parses as JSON of the wrong type must not win over the
    /// literal string an unset `Option<String>` expects.
    #[test]
    fn unset_optional_string_keeps_numeric_looking_values_as_strings() {
        let config = ProviderConfig::Wordpress(wordpress::WordPressConfig::default());
        let json = apply(config, "ANYBUILD_WP_VERSION", "6.4").expect("WP_VERSION=6.4 is accepted");
        assert_eq!(json["wp_version"], serde_json::json!("6.4"));
    }

    /// Coercion must not swallow genuinely malformed input.
    #[test]
    fn unset_optional_bool_still_rejects_non_boolean_values() {
        let config = ProviderConfig::Wordpress(wordpress::WordPressConfig::default());
        let error =
            apply(config, "ANYBUILD_PHPIX", "enabled").expect_err("PHPIX=enabled is not a boolean");
        assert!(
            error
                .to_string()
                .contains("Invalid value for ANYBUILD_PHPIX"),
            "{error}"
        );
    }
}
