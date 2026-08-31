//! Runtime resources embedded into the Anybuild executable.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};

use crate::operation::OperationContext;

const STARLIB_FILES: &[(&str, &[u8])] = &[
    (
        "prelude.bzl",
        include_bytes!("../../resources/starlib/prelude.bzl"),
    ),
    (
        "serve.bzl",
        include_bytes!("../../resources/starlib/serve.bzl"),
    ),
    (
        "tools/go.bzl",
        include_bytes!("../../resources/starlib/tools/go.bzl"),
    ),
    (
        "tools/hugo.bzl",
        include_bytes!("../../resources/starlib/tools/hugo.bzl"),
    ),
    (
        "tools/jekyll.bzl",
        include_bytes!("../../resources/starlib/tools/jekyll.bzl"),
    ),
    (
        "tools/laravel.bzl",
        include_bytes!("../../resources/starlib/tools/laravel.bzl"),
    ),
    (
        "tools/mkdocs.bzl",
        include_bytes!("../../resources/starlib/tools/mkdocs.bzl"),
    ),
    (
        "tools/node.bzl",
        include_bytes!("../../resources/starlib/tools/node.bzl"),
    ),
    (
        "tools/node_static.bzl",
        include_bytes!("../../resources/starlib/tools/node_static.bzl"),
    ),
    (
        "tools/php.bzl",
        include_bytes!("../../resources/starlib/tools/php.bzl"),
    ),
    (
        "tools/python.bzl",
        include_bytes!("../../resources/starlib/tools/python.bzl"),
    ),
    (
        "tools/staticfile.bzl",
        include_bytes!("../../resources/starlib/tools/staticfile.bzl"),
    ),
    (
        "tools/wordpress.bzl",
        include_bytes!("../../resources/starlib/tools/wordpress.bzl"),
    ),
];

const ASSET_FILES: &[(&str, &[u8])] = &[
    (
        "node/optimize-node-modules.sh",
        include_bytes!("../../resources/assets/node/optimize-node-modules.sh"),
    ),
    (
        "php/php.ini",
        include_bytes!("../../resources/assets/php/php.ini"),
    ),
    (
        "wordpress/.htaccess",
        include_bytes!("../../resources/assets/wordpress/.htaccess"),
    ),
    (
        "wordpress/install.sh",
        include_bytes!("../../resources/assets/wordpress/install.sh"),
    ),
    (
        "wordpress/start.php",
        include_bytes!("../../resources/assets/wordpress/start.php"),
    ),
    (
        "wordpress/wp-config.php",
        include_bytes!("../../resources/assets/wordpress/wp-config.php"),
    ),
];

pub struct RuntimeResources {
    pub starlib_dir: PathBuf,
    pub assets_dir: PathBuf,
    _temp_dir: Option<tempfile::TempDir>,
}

pub fn resolve(operation: &OperationContext) -> Result<RuntimeResources> {
    let starlib_override = override_dir(operation, "ANYBUILD_STARLIB", "SHIPIT_STARLIB")?;
    let assets_override = override_dir(operation, "ANYBUILD_ASSETS", "SHIPIT_ASSETS")?;
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

fn override_dir(
    operation: &OperationContext,
    override_var: &str,
    legacy_var: &str,
) -> Result<Option<PathBuf>> {
    let selected = operation
        .environment_var(override_var)
        .map(|path| (override_var, path))
        .or_else(|| {
            operation
                .environment_var(legacy_var)
                .map(|path| (legacy_var, path))
        });
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

    #[cfg(unix)]
    use std::os::unix::fs::{symlink, PermissionsExt};
    #[cfg(unix)]
    use std::process::Command;

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
    fn embedded_lists_cover_packaged_resources() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources");
        assert_eq!(
            STARLIB_FILES
                .iter()
                .map(|(path, _)| (*path).to_owned())
                .collect::<BTreeSet<_>>(),
            source_files(&root.join("starlib"))
        );
        assert_eq!(
            ASSET_FILES
                .iter()
                .map(|(path, _)| (*path).to_owned())
                .collect::<BTreeSet<_>>(),
            source_files(&root.join("assets"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn node_optimizer_removes_dangling_native_binary_links() {
        let resources = resolve(&OperationContext::for_test()).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let modules = temp.path().join("node_modules");
        let bin = modules.join(".bin");
        let esbuild = modules.join("esbuild/bin/esbuild");
        let script = modules.join("tool/cli.js");
        let wasm = modules.join("runtime/module.wasm");
        std::fs::create_dir_all(esbuild.parent().unwrap()).unwrap();
        std::fs::create_dir_all(script.parent().unwrap()).unwrap();
        std::fs::create_dir_all(wasm.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&bin).unwrap();

        std::fs::write(&esbuild, b"\x7fELF\0native").unwrap();
        std::fs::write(&script, b"#!/usr/bin/env node\nconsole.log('ok');\n").unwrap();
        std::fs::write(&wasm, b"\0asm\x01\0\0\0").unwrap();
        for path in [&esbuild, &script, &wasm] {
            let mut permissions = std::fs::metadata(path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(path, permissions).unwrap();
        }
        symlink("../esbuild/bin/esbuild", bin.join("esbuild")).unwrap();
        symlink("../tool/cli.js", bin.join("tool")).unwrap();

        let status = Command::new("bash")
            .arg(resources.assets_dir.join("node/optimize-node-modules.sh"))
            .arg(&modules)
            .status()
            .unwrap();

        assert!(status.success());
        assert!(!esbuild.exists());
        assert!(std::fs::symlink_metadata(bin.join("esbuild")).is_err());
        assert!(script.exists());
        assert!(std::fs::symlink_metadata(bin.join("tool"))
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(wasm.exists());
    }
}
