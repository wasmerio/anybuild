use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use indexmap::IndexMap;

pub fn volumes_dir(shipit_dir: &Path) -> PathBuf {
    shipit_dir.join("volumes")
}

pub fn volume_mappings_path(shipit_dir: &Path) -> PathBuf {
    volumes_dir(shipit_dir).join("mappings.json")
}

/// Load persisted volume name to guest-path mappings.
///
/// Missing mapping files represent a project without volumes. Non-string
/// values retain the Python implementation's `str(value)`-like JSON form.
pub fn load_volume_mappings(shipit_dir: &Path) -> Result<IndexMap<String, String>> {
    let mappings_path = volume_mappings_path(shipit_dir);
    if !mappings_path.is_file() {
        return Ok(IndexMap::new());
    }

    let text = std::fs::read_to_string(&mappings_path)?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("Volume mappings must be a dictionary"))?;

    Ok(object
        .iter()
        .map(|(name, guest_path)| {
            let guest_path = match guest_path {
                serde_json::Value::String(value) => value.clone(),
                other => other.to_string(),
            };
            (name.clone(), guest_path)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_volume_mappings_are_empty() {
        let tmp = tempfile::tempdir().unwrap();

        assert!(load_volume_mappings(tmp.path()).unwrap().is_empty());
    }

    #[test]
    fn volume_mappings_require_a_json_object() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(volumes_dir(tmp.path())).unwrap();
        std::fs::write(volume_mappings_path(tmp.path()), "[]").unwrap();

        let error = load_volume_mappings(tmp.path()).unwrap_err();

        assert_eq!(error.to_string(), "Volume mappings must be a dictionary");
    }

    #[test]
    fn volume_mappings_preserve_order_and_stringify_values() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(volumes_dir(tmp.path())).unwrap();
        std::fs::write(
            volume_mappings_path(tmp.path()),
            r#"{"uploads":"/data/uploads","numeric":42}"#,
        )
        .unwrap();

        let mappings = load_volume_mappings(tmp.path()).unwrap();

        assert_eq!(
            mappings.into_iter().collect::<Vec<_>>(),
            [
                ("uploads".to_owned(), "/data/uploads".to_owned()),
                ("numeric".to_owned(), "42".to_owned())
            ]
        );
    }
}
