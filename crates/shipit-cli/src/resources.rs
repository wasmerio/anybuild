//! Runtime resources embedded into the Shipit executable.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};

const STARLIB_FILES: &[(&str, &[u8])] = &[
    (
        "prelude.shipit",
        include_bytes!("../../../src/shipit/starlib/prelude.shipit"),
    ),
    (
        "serve.shipit",
        include_bytes!("../../../src/shipit/starlib/serve.shipit"),
    ),
    (
        "tools/go.shipit",
        include_bytes!("../../../src/shipit/starlib/tools/go.shipit"),
    ),
    (
        "tools/hugo.shipit",
        include_bytes!("../../../src/shipit/starlib/tools/hugo.shipit"),
    ),
    (
        "tools/jekyll.shipit",
        include_bytes!("../../../src/shipit/starlib/tools/jekyll.shipit"),
    ),
    (
        "tools/laravel.shipit",
        include_bytes!("../../../src/shipit/starlib/tools/laravel.shipit"),
    ),
    (
        "tools/mkdocs.shipit",
        include_bytes!("../../../src/shipit/starlib/tools/mkdocs.shipit"),
    ),
    (
        "tools/node.shipit",
        include_bytes!("../../../src/shipit/starlib/tools/node.shipit"),
    ),
    (
        "tools/node_static.shipit",
        include_bytes!("../../../src/shipit/starlib/tools/node_static.shipit"),
    ),
    (
        "tools/php.shipit",
        include_bytes!("../../../src/shipit/starlib/tools/php.shipit"),
    ),
    (
        "tools/python.shipit",
        include_bytes!("../../../src/shipit/starlib/tools/python.shipit"),
    ),
    (
        "tools/staticfile.shipit",
        include_bytes!("../../../src/shipit/starlib/tools/staticfile.shipit"),
    ),
    (
        "tools/wordpress.shipit",
        include_bytes!("../../../src/shipit/starlib/tools/wordpress.shipit"),
    ),
];

const ASSET_FILES: &[(&str, &[u8])] = &[
    (
        "node/optimize-node-modules.sh",
        include_bytes!("../../../src/shipit/assets/node/optimize-node-modules.sh"),
    ),
    (
        "php/php.ini",
        include_bytes!("../../../src/shipit/assets/php/php.ini"),
    ),
    (
        "wordpress/.htaccess",
        include_bytes!("../../../src/shipit/assets/wordpress/.htaccess"),
    ),
    (
        "wordpress/install.sh",
        include_bytes!("../../../src/shipit/assets/wordpress/install.sh"),
    ),
    (
        "wordpress/start.php",
        include_bytes!("../../../src/shipit/assets/wordpress/start.php"),
    ),
    (
        "wordpress/wp-config.php",
        include_bytes!("../../../src/shipit/assets/wordpress/wp-config.php"),
    ),
];

pub struct RuntimeResources {
    pub starlib_dir: PathBuf,
    pub assets_dir: PathBuf,
    _temp_dir: Option<tempfile::TempDir>,
}

pub fn resolve() -> Result<RuntimeResources> {
    let starlib_override = override_dir("SHIPIT_STARLIB")?;
    let assets_override = override_dir("SHIPIT_ASSETS")?;
    let temp_dir = if starlib_override.is_none() || assets_override.is_none() {
        Some(
            tempfile::Builder::new()
                .prefix("shipit-runtime-")
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

fn override_dir(override_var: &str) -> Result<Option<PathBuf>> {
    if let Some(path) = std::env::var_os(override_var) {
        let path = PathBuf::from(path);
        if !path.is_dir() {
            bail!(
                "{override_var} must point to a directory: {}",
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
            std::fs::read_to_string(starlib_dir.join("tools/python.shipit")).unwrap(),
            include_str!("../../../src/shipit/starlib/tools/python.shipit")
        );
        assert_eq!(
            std::fs::read(assets_dir.join("wordpress/.htaccess")).unwrap(),
            include_bytes!("../../../src/shipit/assets/wordpress/.htaccess")
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
            source_files(&root.join("src/shipit/starlib"))
        );
        assert_eq!(
            embedded_assets,
            source_files(&root.join("src/shipit/assets"))
        );
    }
}
