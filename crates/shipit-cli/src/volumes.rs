//! Port of `src/shipit/volumes.py`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use indexmap::IndexMap;

use shipit_plan::{Serve, Volume};

pub fn get_volumes_dir(src_dir: &Path, shipit_dir: Option<&Path>) -> PathBuf {
    match shipit_dir {
        Some(dir) => dir.join("volumes"),
        None => src_dir.join(".shipit").join("volumes"),
    }
}

pub fn get_volume_mappings_path(src_dir: &Path, shipit_dir: Option<&Path>) -> PathBuf {
    get_volumes_dir(src_dir, shipit_dir).join("mappings.json")
}

/// Port of `build_volumes`: create the volume dirs, link local volumes,
/// and persist the name → guest-path mappings.
pub fn build_volumes(
    src_dir: &Path,
    serve: &Serve,
    shipit_dir: Option<&Path>,
) -> Result<IndexMap<String, String>> {
    let volumes_dir = get_volumes_dir(src_dir, shipit_dir);
    std::fs::create_dir_all(&volumes_dir)?;

    let volumes: &[Volume] = serve.volumes.as_deref().unwrap_or(&[]);
    let mut mappings: IndexMap<String, String> = IndexMap::new();
    for volume in volumes {
        mappings.insert(
            volume.name.clone(),
            volume.serve_path.display().to_string(),
        );
    }
    for volume in volumes {
        std::fs::create_dir_all(&volume.path)?;
        if should_link_local_volume(src_dir, volume, shipit_dir)? {
            link_local_volume(volume)?;
        }
    }

    // json.dumps(mappings, indent=2, sort_keys=True) + "\n"
    let sorted: BTreeMap<&String, &String> = mappings.iter().collect();
    let json = serde_json::to_string_pretty(&sorted)?;
    std::fs::write(
        get_volume_mappings_path(src_dir, shipit_dir),
        format!("{json}\n"),
    )?;
    Ok(mappings)
}

pub fn load_volume_mappings(
    src_dir: &Path,
    shipit_dir: Option<&Path>,
) -> Result<IndexMap<String, String>> {
    let mappings_path = get_volume_mappings_path(src_dir, shipit_dir);
    if !mappings_path.is_file() {
        return Ok(IndexMap::new());
    }

    let text = std::fs::read_to_string(&mappings_path)?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("Volume mappings must be a dictionary"))?;

    let mut mappings = IndexMap::new();
    for (name, guest_path) in object {
        let guest_path = match guest_path {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        mappings.insert(name.clone(), guest_path);
    }
    Ok(mappings)
}

pub fn parse_cli_volume_mappings(volume_specs: &[String]) -> Result<IndexMap<String, String>> {
    let mut mappings = IndexMap::new();
    for spec in volume_specs {
        let (name, guest_path) = parse_volume_spec(spec)?;
        mappings.insert(name, guest_path);
    }
    Ok(mappings)
}

pub fn merge_volume_mappings(
    mapping_sets: &[IndexMap<String, String>],
) -> IndexMap<String, String> {
    let mut merged = IndexMap::new();
    for mappings in mapping_sets {
        for (name, guest_path) in mappings {
            merged.insert(name.clone(), guest_path.clone());
        }
    }
    merged
}

fn parse_volume_spec(spec: &str) -> Result<(String, String)> {
    let Some((name, guest_path)) = spec.split_once(':') else {
        bail!("Invalid volume mapping '{spec}'. Expected NAME:/guest/path");
    };
    if name.is_empty() {
        bail!("Invalid volume mapping '{spec}'. Volume name cannot be empty");
    }
    if guest_path.is_empty() {
        bail!("Invalid volume mapping '{spec}'. Guest path cannot be empty");
    }
    if !Path::new(guest_path).is_absolute() {
        bail!("Invalid volume mapping '{spec}'. Guest path must be absolute");
    }
    Ok((name.to_owned(), guest_path.to_owned()))
}

fn absolute(path: &Path) -> Result<PathBuf> {
    Ok(std::path::absolute(path)?)
}

fn resolve_or_lexical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| {
        // Path.resolve(strict=False) on a missing path: absolute + normalized.
        let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        let mut out = PathBuf::new();
        for component in abs.components() {
            match component {
                std::path::Component::ParentDir => {
                    out.pop();
                }
                std::path::Component::CurDir => {}
                other => out.push(other),
            }
        }
        out
    })
}

fn should_link_local_volume(
    src_dir: &Path,
    volume: &Volume,
    shipit_dir: Option<&Path>,
) -> Result<bool> {
    let shipit_dir = match shipit_dir {
        Some(dir) => absolute(dir)?,
        None => absolute(&src_dir.join(".shipit"))?,
    };
    Ok(volume.serve_path.is_absolute() && volume.serve_path.starts_with(&shipit_dir))
}

fn dir_is_empty(path: &Path) -> Result<bool> {
    Ok(std::fs::read_dir(path)?.next().is_none())
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let src = entry.path();
        let dst = target.join(entry.file_name());
        if std::fs::metadata(&src)?.is_dir() {
            copy_dir_recursive(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

/// Port of `_link_local_volume`: replace the guest path with a symlink into
/// the volumes dir, preserving any existing content on first link.
fn link_local_volume(volume: &Volume) -> Result<()> {
    let source = absolute(&volume.path)?;
    let target = &volume.serve_path;

    if target.is_symlink() {
        if resolve_or_lexical(target) == resolve_or_lexical(&source) {
            return Ok(());
        }
        std::fs::remove_file(target)?;
    } else if target.exists() {
        if target.is_dir() {
            if dir_is_empty(&source)? {
                copy_dir_recursive(target, &source)?;
            }
            std::fs::remove_dir_all(target)?;
        } else {
            if dir_is_empty(&source)? {
                let name = target
                    .file_name()
                    .ok_or_else(|| anyhow!("Invalid volume target: {}", target.display()))?;
                std::fs::copy(target, source.join(name))?;
            }
            std::fs::remove_file(target)?;
        }
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::os::unix::fs::symlink(&source, target)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_volume_mappings_valid() {
        let mappings = parse_cli_volume_mappings(&[
            "data:/data".to_owned(),
            "wp-content:/app/wp-content".to_owned(),
            // colons allowed in guest path (split on the first one)
            "odd:/mnt/a:b".to_owned(),
        ])
        .unwrap();
        assert_eq!(mappings.get("data").unwrap(), "/data");
        assert_eq!(mappings.get("wp-content").unwrap(), "/app/wp-content");
        assert_eq!(mappings.get("odd").unwrap(), "/mnt/a:b");
    }

    #[test]
    fn parse_cli_volume_mappings_errors() {
        let no_colon = parse_cli_volume_mappings(&["data".to_owned()]).unwrap_err();
        assert_eq!(
            no_colon.to_string(),
            "Invalid volume mapping 'data'. Expected NAME:/guest/path"
        );
        let empty_name = parse_cli_volume_mappings(&[":/data".to_owned()]).unwrap_err();
        assert_eq!(
            empty_name.to_string(),
            "Invalid volume mapping ':/data'. Volume name cannot be empty"
        );
        let empty_path = parse_cli_volume_mappings(&["data:".to_owned()]).unwrap_err();
        assert_eq!(
            empty_path.to_string(),
            "Invalid volume mapping 'data:'. Guest path cannot be empty"
        );
        let relative = parse_cli_volume_mappings(&["data:rel/path".to_owned()]).unwrap_err();
        assert_eq!(
            relative.to_string(),
            "Invalid volume mapping 'data:rel/path'. Guest path must be absolute"
        );
    }

    #[test]
    fn merge_volume_mappings_later_wins() {
        let mut a = IndexMap::new();
        a.insert("data".to_owned(), "/old".to_owned());
        a.insert("keep".to_owned(), "/keep".to_owned());
        let mut b = IndexMap::new();
        b.insert("data".to_owned(), "/new".to_owned());
        let merged = merge_volume_mappings(&[a, b]);
        assert_eq!(merged.get("data").unwrap(), "/new");
        assert_eq!(merged.get("keep").unwrap(), "/keep");
    }

    #[test]
    fn load_volume_mappings_missing_file_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let mappings = load_volume_mappings(tmp.path(), None).unwrap();
        assert!(mappings.is_empty());
    }

    #[test]
    fn build_and_load_roundtrip_sorted() {
        let tmp = tempfile::tempdir().unwrap();
        let serve = Serve {
            name: "app".to_owned(),
            provider: "test".to_owned(),
            build: vec![],
            deps: vec![],
            commands: IndexMap::new(),
            cwd: None,
            prepare: None,
            mounts: None,
            volumes: Some(vec![
                Volume {
                    name: "zeta".to_owned(),
                    path: tmp.path().join(".shipit/volumes/zeta"),
                    serve_path: PathBuf::from("/data/zeta"),
                },
                Volume {
                    name: "alpha".to_owned(),
                    path: tmp.path().join(".shipit/volumes/alpha"),
                    serve_path: PathBuf::from("/data/alpha"),
                },
            ]),
            env: None,
            services: None,
        };
        build_volumes(tmp.path(), &serve, None).unwrap();
        let raw = std::fs::read_to_string(get_volume_mappings_path(tmp.path(), None)).unwrap();
        // sort_keys=True + indent=2 + trailing newline
        assert_eq!(
            raw,
            "{\n  \"alpha\": \"/data/alpha\",\n  \"zeta\": \"/data/zeta\"\n}\n"
        );
        let loaded = load_volume_mappings(tmp.path(), None).unwrap();
        assert_eq!(loaded.get("zeta").unwrap(), "/data/zeta");
        assert!(tmp.path().join(".shipit/volumes/alpha").is_dir());
    }
}
