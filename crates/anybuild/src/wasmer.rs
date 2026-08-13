use std::path::Path;

use anyhow::Result;
use serde_yaml::Value as YamlValue;

pub(crate) fn path_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub(crate) fn yaml_str(value: &str) -> YamlValue {
    YamlValue::String(value.to_owned())
}

/// PyYAML's `yaml.dump`: block style with keys sorted at every level.
pub(crate) fn dump_yaml_sorted(value: &YamlValue) -> Result<String> {
    fn sort(value: &YamlValue) -> YamlValue {
        match value {
            YamlValue::Mapping(map) => {
                let mut entries: Vec<(YamlValue, YamlValue)> = map
                    .iter()
                    .map(|(key, value)| (key.clone(), sort(value)))
                    .collect();
                entries.sort_by_key(|entry| yaml_key(&entry.0));
                YamlValue::Mapping(entries.into_iter().collect())
            }
            YamlValue::Sequence(items) => YamlValue::Sequence(items.iter().map(sort).collect()),
            other => other.clone(),
        }
    }

    fn yaml_key(value: &YamlValue) -> String {
        match value {
            YamlValue::String(value) => value.clone(),
            other => serde_yaml::to_string(other).unwrap_or_default(),
        }
    }

    Ok(serde_yaml::to_string(&sort(value))?)
}
