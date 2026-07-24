//! Volume parsing and normalization.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use indexmap::IndexMap;

use crate::common::paths::normalize_absolute;
use crate::common::volumes::{
    load_volume_mappings as load_persisted_volume_mappings, volume_mappings_path, volumes_dir,
};
use crate::plan::{Serve, Volume};

fn effective_anybuild_dir(src_dir: &Path, anybuild_dir: Option<&Path>) -> PathBuf {
    anybuild_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| src_dir.join(".anybuild"))
}

pub fn get_volumes_dir(src_dir: &Path, anybuild_dir: Option<&Path>) -> PathBuf {
    volumes_dir(&effective_anybuild_dir(src_dir, anybuild_dir))
}

pub fn get_volume_mappings_path(src_dir: &Path, anybuild_dir: Option<&Path>) -> PathBuf {
    volume_mappings_path(&effective_anybuild_dir(src_dir, anybuild_dir))
}

/// Port of `build_volumes`: create the volume dirs, link local volumes,
/// and persist the name → guest-path mappings.
pub fn build_volumes(
    src_dir: &Path,
    serve: &Serve,
    anybuild_dir: Option<&Path>,
) -> Result<IndexMap<String, String>> {
    let volumes_dir = get_volumes_dir(src_dir, anybuild_dir);
    std::fs::create_dir_all(&volumes_dir)?;

    let volumes: &[Volume] = serve.volumes.as_deref().unwrap_or(&[]);
    let mut mappings: IndexMap<String, String> = IndexMap::new();
    for volume in volumes {
        mappings.insert(volume.name.clone(), volume.serve_path.display().to_string());
    }
    for volume in volumes {
        std::fs::create_dir_all(&volume.path)?;
        if should_link_local_volume(src_dir, volume, anybuild_dir)? {
            link_local_volume(volume)?;
        }
    }

    // json.dumps(mappings, indent=2, sort_keys=True) + "\n"
    let sorted: BTreeMap<&String, &String> = mappings.iter().collect();
    let json = serde_json::to_string_pretty(&sorted)?;
    std::fs::write(
        get_volume_mappings_path(src_dir, anybuild_dir),
        format!("{json}\n"),
    )?;
    Ok(mappings)
}

pub fn load_volume_mappings(
    src_dir: &Path,
    anybuild_dir: Option<&Path>,
) -> Result<IndexMap<String, String>> {
    load_persisted_volume_mappings(&effective_anybuild_dir(src_dir, anybuild_dir))
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

fn resolve_or_normalize(path: &Path) -> Result<PathBuf> {
    Ok(std::fs::canonicalize(path).or_else(|_| normalize_absolute(path))?)
}

fn should_link_local_volume(
    src_dir: &Path,
    volume: &Volume,
    anybuild_dir: Option<&Path>,
) -> Result<bool> {
    let anybuild_dir = match anybuild_dir {
        Some(dir) => absolute(dir)?,
        None => absolute(&src_dir.join(".anybuild"))?,
    };
    Ok(volume.serve_path.is_absolute() && volume.serve_path.starts_with(&anybuild_dir))
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
        if resolve_or_normalize(target)? == resolve_or_normalize(&source)? {
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
    #[cfg(unix)]
    std::os::unix::fs::symlink(&source, target)?;
    // The volume source is always a directory.
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&source, target)?;
    Ok(())
}
