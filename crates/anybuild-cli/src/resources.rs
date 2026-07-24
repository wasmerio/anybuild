//! Runtime resources embedded into the Anybuild executable.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};

const STARLIB_FILES: &[(&str, &[u8])] = &[
    (
        "prelude.bzl",
        include_bytes!("../../../resources/starlib/prelude.bzl"),
    ),
    (
        "serve.bzl",
        include_bytes!("../../../resources/starlib/serve.bzl"),
    ),
    (
        "tools/go.bzl",
        include_bytes!("../../../resources/starlib/tools/go.bzl"),
    ),
    (
        "tools/hugo.bzl",
        include_bytes!("../../../resources/starlib/tools/hugo.bzl"),
    ),
    (
        "tools/jekyll.bzl",
        include_bytes!("../../../resources/starlib/tools/jekyll.bzl"),
    ),
    (
        "tools/laravel.bzl",
        include_bytes!("../../../resources/starlib/tools/laravel.bzl"),
    ),
    (
        "tools/mkdocs.bzl",
        include_bytes!("../../../resources/starlib/tools/mkdocs.bzl"),
    ),
    (
        "tools/node.bzl",
        include_bytes!("../../../resources/starlib/tools/node.bzl"),
    ),
    (
        "tools/node_static.bzl",
        include_bytes!("../../../resources/starlib/tools/node_static.bzl"),
    ),
    (
        "tools/php.bzl",
        include_bytes!("../../../resources/starlib/tools/php.bzl"),
    ),
    (
        "tools/python.bzl",
        include_bytes!("../../../resources/starlib/tools/python.bzl"),
    ),
    (
        "tools/staticfile.bzl",
        include_bytes!("../../../resources/starlib/tools/staticfile.bzl"),
    ),
    (
        "tools/wordpress.bzl",
        include_bytes!("../../../resources/starlib/tools/wordpress.bzl"),
    ),
];

const ASSET_FILES: &[(&str, &[u8])] = &[
    (
        "node/optimize-node-modules.sh",
        include_bytes!("../../../resources/assets/node/optimize-node-modules.sh"),
    ),
    (
        "php/php.ini",
        include_bytes!("../../../resources/assets/php/php.ini"),
    ),
    (
        "wordpress/.htaccess",
        include_bytes!("../../../resources/assets/wordpress/.htaccess"),
    ),
    (
        "wordpress/install.sh",
        include_bytes!("../../../resources/assets/wordpress/install.sh"),
    ),
    (
        "wordpress/start.php",
        include_bytes!("../../../resources/assets/wordpress/start.php"),
    ),
    (
        "wordpress/wp-config.php",
        include_bytes!("../../../resources/assets/wordpress/wp-config.php"),
    ),
];

pub struct RuntimeResources {
    pub starlib_dir: PathBuf,
    pub assets_dir: PathBuf,
    _temp_dir: Option<tempfile::TempDir>,
}

pub fn resolve() -> Result<RuntimeResources> {
    let starlib_override = override_dir("ANYBUILD_STARLIB", "SHIPIT_STARLIB")?;
    let assets_override = override_dir("ANYBUILD_ASSETS", "SHIPIT_ASSETS")?;
    let temp_dir = if starlib_override.is_none() || assets_override.is_none() {
        Some(
            tempfile::Builder::new()
                .prefix("anybuild-runtime-")
                .tempdir()?,
        )
    } else {
        None
    };
    let temp_path = temp_dir.as_ref().map(|temp| temp.path());

    let starlib_dir = match starlib_override {
        Some(path) => path,
        None => {
            let path = temp_path
                .ok_or_else(|| anyhow!("temporary resource directory was not created"))?
                .join("starlib");
            materialize(&path, STARLIB_FILES)?;
            path
        }
    };
    let assets_dir = match assets_override {
        Some(path) => path,
        None => {
            let path = temp_path
                .ok_or_else(|| anyhow!("temporary resource directory was not created"))?
                .join("assets");
            materialize(&path, ASSET_FILES)?;
            path
        }
    };

    Ok(RuntimeResources {
        starlib_dir,
        assets_dir,
        _temp_dir: temp_dir,
    })
}

fn override_dir(override_var: &str, legacy_var: &str) -> Result<Option<PathBuf>> {
    let selected = std::env::var_os(override_var)
        .map(|path| (override_var, path))
        .or_else(|| std::env::var_os(legacy_var).map(|path| (legacy_var, path)));
    if let Some((selected_var, path)) = selected {
        let path = PathBuf::from(path);
        if !path.is_dir() {
            bail!(
                "{selected_var} must point to a directory: {}",
                path.display()
            );
        }
        return Ok(Some(path));
    }
    Ok(None)
}

fn materialize(root: &Path, files: &[(&str, &[u8])]) -> Result<()> {
    for (relative, contents) in files {
        let target = root.join(relative);
        if std::fs::read(&target).is_ok_and(|existing| existing == *contents) {
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("creating embedded resource directory {}", parent.display())
            })?;
        }
        std::fs::write(&target, contents)
            .with_context(|| format!("writing embedded resource {}", target.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn source_files(root: &Path) -> BTreeSet<String> {
        fn visit(root: &Path, path: &Path, files: &mut BTreeSet<String>) {
            for entry in std::fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    visit(root, &path, files);
                } else {
                    files.insert(
                        path.strip_prefix(root)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }

        let mut files = BTreeSet::new();
        visit(root, root, &mut files);
        files
    }

    #[test]
    fn embedded_resources_materialize_without_the_source_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let starlib_dir = tmp.path().join("starlib");
        let assets_dir = tmp.path().join("assets");
        materialize(&starlib_dir, STARLIB_FILES).unwrap();
        materialize(&assets_dir, ASSET_FILES).unwrap();

        assert_eq!(
            std::fs::read_to_string(starlib_dir.join("tools/python.bzl")).unwrap(),
            include_str!("../../../resources/starlib/tools/python.bzl")
        );
        assert_eq!(
            std::fs::read(assets_dir.join("wordpress/.htaccess")).unwrap(),
            include_bytes!("../../../resources/assets/wordpress/.htaccess")
        );
    }

    #[test]
    fn embedded_resource_lists_cover_the_source_directories() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let embedded_starlib: BTreeSet<String> = STARLIB_FILES
            .iter()
            .map(|(path, _)| (*path).to_owned())
            .collect();
        let embedded_assets: BTreeSet<String> = ASSET_FILES
            .iter()
            .map(|(path, _)| (*path).to_owned())
            .collect();

        assert_eq!(
            embedded_starlib,
            source_files(&root.join("resources/starlib"))
        );
        assert_eq!(
            embedded_assets,
            source_files(&root.join("resources/assets"))
        );
    }
}
