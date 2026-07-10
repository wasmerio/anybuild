//! Project path resolution (port of the top of `cli.py`).

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};

#[derive(Debug, Clone)]
pub struct ProjectPaths {
    pub workspace_root: PathBuf,
    pub app_path: PathBuf,
    pub subdir: Option<String>,
}

pub fn shipit_subdir_slug(subdir: &str) -> String {
    let replaced = subdir.replace('/', "-");
    let re = regex_lite(r"[^A-Za-z0-9._-]+");
    let slug = re.replace_all(&replaced, "-");
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "app".to_owned()
    } else {
        trimmed.to_owned()
    }
}

// A tiny local regex shim to avoid pulling the regex crate into the CLI
// for one pattern: replaces runs of non-[A-Za-z0-9._-] with `-`.
fn regex_lite(_pattern: &str) -> SlugRe {
    SlugRe
}

struct SlugRe;

impl SlugRe {
    fn replace_all<'a>(&self, text: &'a str, replacement: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut in_run = false;
        for ch in text.chars() {
            let keep = ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-');
            if keep {
                out.push(ch);
                in_run = false;
            } else if !in_run {
                out.push_str(replacement);
                in_run = true;
            }
        }
        out
    }
}

pub fn default_shipit_path(paths: &ProjectPaths) -> PathBuf {
    match &paths.subdir {
        None => paths.workspace_root.join("Shipit"),
        Some(subdir) => paths
            .workspace_root
            .join(format!("Shipit.{}", shipit_subdir_slug(subdir))),
    }
}

pub fn default_shipit_dir(paths: &ProjectPaths) -> PathBuf {
    let shipit_dir = paths.workspace_root.join(".shipit");
    match &paths.subdir {
        None => shipit_dir,
        Some(subdir) => shipit_dir.join(shipit_subdir_slug(subdir)),
    }
}

pub fn resolve_project_paths(path: &Path, subdir: Option<&str>) -> Result<ProjectPaths> {
    let workspace_root = path
        .canonicalize()
        .map_err(|_| anyhow!("The path {} does not exist", path.display()))?;
    let Some(subdir) = subdir else {
        return Ok(ProjectPaths {
            app_path: workspace_root.clone(),
            workspace_root,
            subdir: None,
        });
    };
    if Path::new(subdir).is_absolute() {
        bail!("--subdir must be relative to the project path");
    }
    let subdir_text = subdir.trim_matches('/');
    if subdir_text.is_empty() || subdir_text == "." {
        return Ok(ProjectPaths {
            app_path: workspace_root.clone(),
            workspace_root,
            subdir: None,
        });
    }
    let app_path = workspace_root
        .join(subdir_text)
        .canonicalize()
        .map_err(|_| anyhow!("--subdir does not exist: {subdir_text}"))?;
    let normalized = app_path
        .strip_prefix(&workspace_root)
        .map_err(|_| anyhow!("--subdir must stay inside the project path"))?
        .to_string_lossy()
        .replace('\\', "/");
    if !app_path.is_dir() {
        bail!("--subdir is not a directory: {subdir_text}");
    }
    Ok(ProjectPaths {
        workspace_root,
        app_path,
        subdir: Some(normalized),
    })
}

/// Port of `read_shipit_subdir`: the `app_subdir = "..."` marker line.
pub fn read_shipit_subdir(shipit_file: &Path) -> Option<String> {
    let text = std::fs::read_to_string(shipit_file).ok()?;
    for line in text.lines() {
        let trimmed = line.trim_end();
        let Some(rest) = trimmed.strip_prefix("app_subdir") else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let literal = rest.trim();
        if literal.starts_with('"') && literal.ends_with('"') {
            if let Ok(serde_json::Value::String(value)) = serde_json::from_str(literal) {
                if !value.is_empty() {
                    return Some(value);
                }
                return None;
            }
        }
    }
    None
}

pub fn get_shipit_path(
    paths: &ProjectPaths,
    shipit_path: Option<&Path>,
) -> Result<PathBuf> {
    match shipit_path {
        None => {
            let default = default_shipit_path(paths);
            if !default.exists() {
                let mut command =
                    format!("shipit generate {}", paths.workspace_root.display());
                if let Some(subdir) = &paths.subdir {
                    command = format!("{command} --subdir={subdir}");
                }
                bail!(
                    "Shipit file not found at {}. Run `{command}` to create it.",
                    default.display()
                );
            }
            Ok(default)
        }
        Some(path) => {
            if !path.exists() {
                bail!(
                    "Shipit file not found at {}. Run `shipit generate {} -o {}` to create it.",
                    path.display(),
                    paths.workspace_root.display(),
                    path.display()
                );
            }
            Ok(path.to_path_buf())
        }
    }
}
