//! Project path resolution (port of the top of `cli.py`).

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};

#[derive(Debug, Clone)]
pub struct ProjectPaths {
    pub workspace_root: PathBuf,
    pub app_path: PathBuf,
    pub subdir: Option<String>,
}

pub fn anybuild_subdir_slug(subdir: &str) -> String {
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
    fn replace_all(&self, text: &str, replacement: &str) -> String {
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

pub fn default_anybuild_path(paths: &ProjectPaths) -> PathBuf {
    match &paths.subdir {
        None => paths.workspace_root.join("Anybuild"),
        Some(subdir) => paths
            .workspace_root
            .join(format!("Anybuild.{}", anybuild_subdir_slug(subdir))),
    }
}

pub fn legacy_anybuild_path(paths: &ProjectPaths) -> PathBuf {
    match &paths.subdir {
        None => paths.workspace_root.join("Shipit"),
        Some(subdir) => paths
            .workspace_root
            .join(format!("Shipit.{}", anybuild_subdir_slug(subdir))),
    }
}

pub fn migrate_legacy_anybuild(paths: &ProjectPaths) -> Result<Option<PathBuf>> {
    let current = default_anybuild_path(paths);
    if current.exists() {
        return Ok(Some(current));
    }
    let legacy = legacy_anybuild_path(paths);
    if !legacy.exists() {
        return Ok(None);
    }
    std::fs::rename(&legacy, &current).map_err(|err| {
        anyhow!(
            "Could not rename legacy {} to {}: {err}",
            legacy.display(),
            current.display()
        )
    })?;
    eprintln!(
        "Renamed legacy {} file to {}.",
        legacy
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Shipit"),
        current
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Anybuild")
    );
    Ok(Some(current))
}

pub fn default_anybuild_dir(paths: &ProjectPaths) -> PathBuf {
    let anybuild_dir = paths.workspace_root.join(".anybuild");
    match &paths.subdir {
        None => anybuild_dir,
        Some(subdir) => anybuild_dir.join(anybuild_subdir_slug(subdir)),
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

/// Port of `read_anybuild_subdir`: the `app_subdir = "..."` marker line.
pub fn read_anybuild_subdir(anybuild_file: &Path) -> Option<String> {
    let text = std::fs::read_to_string(anybuild_file).ok()?;
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

pub fn get_anybuild_path(paths: &ProjectPaths, anybuild_path: Option<&Path>) -> Result<PathBuf> {
    match anybuild_path {
        None => {
            let default = default_anybuild_path(paths);
            if migrate_legacy_anybuild(paths)?.is_none() {
                let mut command = format!("anybuild generate {}", paths.workspace_root.display());
                if let Some(subdir) = &paths.subdir {
                    command = format!("{command} --subdir={subdir}");
                }
                bail!(
                    "Anybuild file not found at {}. Run `{command}` to create it.",
                    default.display()
                );
            }
            Ok(default)
        }
        Some(path) => {
            if !path.exists() {
                bail!(
                    "Anybuild file not found at {}. Run `anybuild generate {} -o {}` to create it.",
                    path.display(),
                    paths.workspace_root.display(),
                    path.display()
                );
            }
            Ok(path.to_path_buf())
        }
    }
}

#[cfg(test)]
mod subdir_tests {
    //! Port of `tests/test_subdir.py`, driving the CLI's functions directly
    //! (`generate`, `resolve_project_context`, `load_env_files`, and the
    //! path helpers) rather than the binary.
    //!
    //! Deduped against existing gates (not re-ported here):
    //! - pnpm node subdir deploy export (`pnpm deploy --filter …`, the
    //!   pnpm_config_* env, optimize-deps, no `cp -R`/prune): pinned by the
    //!   node-pnpm-workspace-subdir config fixture + plan snapshot.
    //! - go subdir build-mount rewrites (GOPATH under temp/apps/site, the
    //!   binary-only copy, the /app start suffix): synthetic__go_subdir
    //!   config fixture + plan snapshot.
    //! - python subdir temp-mount staging (temp workdirs, uv add with no
    //!   input narrowing, `cp -R . <app>`): synthetic__python_subdir config
    //!   fixture + plan snapshot; the generated-file shape is the same
    //!   loader path asserted below for node/static.
    //! - static subdir evaluation (flat static_app mount, copy from
    //!   apps/site/public): synthetic__static_subdir fixture + snapshot
    //!   (the generated-file shape IS asserted below).
    //! - generated subdir content for node apps: the goldens test
    //!   (node-npm-file-subdir / node-pnpm-workspace-subdir examples);
    //!   the default *placement* (Anybuild.<slug> naming) is asserted here
    //!   because goldens always passes --out.

    use std::path::{Path, PathBuf};

    use anybuild_plan::{Serve, Step};
    use anybuild_providers::ProviderConfig;
    use indexmap::IndexMap;

    use super::{default_anybuild_dir, resolve_project_paths};
    use crate::commands::build::load_env_files;
    use crate::context::{
        resolve_project_context, CommandOverrides, EnvironmentOptions, ProjectContext,
    };
    use crate::SharedProjectArgs;

    fn no_overrides() -> CommandOverrides {
        CommandOverrides {
            start_command: None,
            install_command: None,
            build_command: None,
            serve_port: None,
            use_provider: None,
            config: None,
        }
    }

    /// The generate command's function entrypoint (what `anybuild generate
    /// <path> --subdir=<subdir>` runs).
    fn generate(workspace: &Path, subdir: &str) {
        crate::commands::generate::run(
            SharedProjectArgs {
                path: workspace.to_path_buf(),
                subdir: Some(subdir.to_owned()),
                ..Default::default()
            },
            None,
        )
        .unwrap();
    }

    fn resolve(
        workspace: &Path,
        subdir: Option<&str>,
        anybuild_path: Option<&Path>,
    ) -> ProjectContext {
        resolve_project_context(
            workspace,
            subdir,
            anybuild_path,
            &no_overrides(),
            &EnvironmentOptions::default(),
        )
        .unwrap()
    }

    fn build_mount(ctx: &ProjectContext, name: &str) -> PathBuf {
        ctx.build_backend.borrow().get_build_mount_path(name)
    }

    fn run_commands(serve: &Serve) -> Vec<&str> {
        serve
            .build
            .iter()
            .filter_map(|step| match step {
                Step::Run(run) => Some(run.command.as_str()),
                _ => None,
            })
            .collect()
    }

    fn workdirs(serve: &Serve) -> Vec<&Path> {
        serve
            .build
            .iter()
            .filter_map(|step| match step {
                Step::Workdir(workdir) => Some(workdir.path.as_path()),
                _ => None,
            })
            .collect()
    }

    fn mount_names(serve: &Serve) -> Vec<&str> {
        serve
            .mounts
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|mount| mount.name.as_str())
            .collect()
    }

    fn write(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn write_node_app(root: &Path, subdir: &str) -> PathBuf {
        let app = root.join(subdir);
        write(
            &app.join("package.json"),
            &(serde_json::json!({
                "scripts": {"start": "node server.js"},
                "dependencies": {"express": "^5.1.0"},
            })
            .to_string()
                + "\n"),
        );
        write(&app.join("package-lock.json"), "{}\n");
        write(&app.join("server.js"), "console.log('ok')\n");
        app
    }

    fn write_pnpm_static_workspace(root: &Path) -> PathBuf {
        let app = root.join("apps/site");
        write(
            &root.join("package.json"),
            &(serde_json::json!({"name": "workspace", "private": true}).to_string() + "\n"),
        );
        write(&root.join("package-lock.json"), "{}\n");
        write(
            &root.join("pnpm-workspace.yaml"),
            "packages:\n  - apps/*\n  - packages/*\n",
        );
        write(
            &app.join("package.json"),
            &(serde_json::json!({
                "name": "@workspace/site",
                "scripts": {"build": "astro build"},
                "dependencies": {
                    "astro": "^6.4.4",
                    "@workspace/shared": "workspace:*",
                    "@workspace/ui": "workspace:*",
                },
            })
            .to_string()
                + "\n"),
        );
        write(
            &root.join("packages/ui/package.json"),
            &(serde_json::json!({"name": "@workspace/ui"}).to_string() + "\n"),
        );
        write(
            &root.join("packages/shared/package.json"),
            &(serde_json::json!({"name": "@workspace/shared"}).to_string() + "\n"),
        );
        app
    }

    #[test]
    fn test_generate_subdir_anybuild_uses_plain_mounts() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path();
        write_node_app(workspace, "apps/site");

        generate(workspace, "apps/site");

        let anybuild_file = workspace.join("Anybuild.apps-site");
        assert!(anybuild_file.is_file());
        assert!(!workspace.join("Anybuild").exists());
        assert!(!workspace.join("apps/site/Anybuild").exists());
        let anybuild = std::fs::read_to_string(&anybuild_file).unwrap();
        assert!(anybuild.contains("app_subdir = \"apps/site\""));

        let ctx = resolve(workspace, Some("apps/site"), None);
        // test_plan_accepts_subdir_and_reports_app_provider's decisive
        // assertion: the reported provider is the app's, not the root's.
        assert_eq!(ctx.provider, "node");
        let build_path = build_mount(&ctx, "build");
        let app_path = build_mount(&ctx, "app");
        let serve = &ctx.serve;

        // Staged in the build mount, entering the subdir; served from the
        // flat app.
        assert_eq!(
            workdirs(serve),
            vec![build_path.as_path(), build_path.join("apps/site").as_path()]
        );
        assert!(serve.build.iter().any(|step| matches!(
            step,
            Step::Copy(copy)
                if copy.source == "."
                    && copy.ignore.as_deref()
                        == Some(&[".git".to_owned(), "node_modules".to_owned()][..])
        )));
        // The workspace lockfile is staged from the subdir path.
        assert!(serve.build.iter().any(|step| matches!(
            step,
            Step::Copy(copy)
                if copy.source == "apps/site/package-lock.json"
                    && copy.target == "package-lock.json"
        )));
        let install_step = serve
            .build
            .iter()
            .find_map(|step| match step {
                Step::Run(run) if run.command.starts_with("npm install") => Some(run),
                _ => None,
            })
            .expect("npm install step");
        assert_eq!(install_step.inputs, None);
        assert!(run_commands(serve).contains(&format!("cp -RL . {}", app_path.display()).as_str()));
        assert!(serve
            .cwd
            .as_deref()
            .is_some_and(|cwd| cwd.ends_with("/app")));
        assert_eq!(mount_names(serve), vec!["app"]);
    }

    #[test]
    fn test_generate_subdir_anybuild_files_do_not_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path();
        write_node_app(workspace, "apps/dashboard");
        write_node_app(workspace, "apps/site");
        write_node_app(workspace, "apps/docs");

        for subdir in ["apps/dashboard", "apps/site", "apps/docs"] {
            generate(workspace, subdir);
        }

        let dashboard = std::fs::read_to_string(workspace.join("Anybuild.apps-dashboard")).unwrap();
        let site = std::fs::read_to_string(workspace.join("Anybuild.apps-site")).unwrap();
        let docs = std::fs::read_to_string(workspace.join("Anybuild.apps-docs")).unwrap();

        assert!(dashboard.contains("app_subdir = \"apps/dashboard\""));
        assert!(site.contains("app_subdir = \"apps/site\""));
        assert!(docs.contains("app_subdir = \"apps/docs\""));
        assert!(!workspace.join("Anybuild").exists());
    }

    #[test]
    fn test_generate_subdir_inherits_workspace_package_manager() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path();
        write_pnpm_static_workspace(workspace);

        generate(workspace, "apps/site");

        let ctx = resolve(workspace, Some("apps/site"), None);
        let serve = &ctx.serve;
        // The workspace package manager (pnpm) wins over the app-level
        // default.
        assert!(serve.build.iter().any(|step| matches!(
            step,
            Step::Use(use_step)
                if use_step.dependencies.iter().any(|pkg| pkg.name == "pnpm")
        )));
        let commands = run_commands(serve);
        assert!(commands
            .iter()
            .any(|command| command.starts_with("pnpm install")));
        assert!(!commands
            .iter()
            .any(|command| command.starts_with("npm install")));
        let static_dir = match &ctx.provider_config {
            ProviderConfig::NodeStatic(config) => {
                config.static_dir.clone().expect("static_dir set")
            }
            other => panic!("expected a node-static config, got {other:?}"),
        };
        let build_step = serve
            .build
            .iter()
            .find_map(|step| match step {
                Step::Run(run) if run.command == "pnpm run build" => Some(run),
                _ => None,
            })
            .expect("pnpm run build step");
        assert_eq!(build_step.outputs, Some(vec![static_dir]));
    }

    #[test]
    fn test_generate_subdir_static_provider_anybuild_shape() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path();
        write(
            &workspace.join("apps/site/public/index.html"),
            "<h1>ok</h1>\n",
        );

        generate(workspace, "apps/site");

        let anybuild = std::fs::read_to_string(workspace.join("Anybuild.apps-site")).unwrap();
        assert!(anybuild.contains("build = staticfile_build(config)"));
        assert!(anybuild.contains("staticfile_serve(config, build, name = \"site\")"));
        assert!(anybuild.contains("app_subdir = \"apps/site\""));
        // The flat static_app serve mount and the apps/site/public copy are
        // pinned by the synthetic__static_subdir fixture + plan snapshot.
    }

    #[test]
    fn test_subdir_recovered_from_generated_anybuild_path() {
        // Merges test_subdir_anybuild_evaluates_to_subdir_build_and_runtime_paths
        // and test_build_recovers_subdir_from_generated_anybuild: an explicit
        // --anybuild-path with no --subdir recovers the subdir from the
        // app_subdir marker and evaluates the subdir plan.
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path();
        write_node_app(workspace, "apps/site");
        generate(workspace, "apps/site");

        let anybuild_file = workspace.join("Anybuild.apps-site");
        let ctx = resolve(workspace, None, Some(&anybuild_file));

        assert_eq!(ctx.paths.subdir.as_deref(), Some("apps/site"));
        assert_eq!(
            ctx.anybuild_dir,
            ctx.paths.workspace_root.join(".anybuild").join("apps-site")
        );
        let serve = &ctx.serve;
        assert!(serve
            .cwd
            .as_deref()
            .is_some_and(|cwd| cwd.ends_with("/app")));
        let build_path = build_mount(&ctx, "build");
        match &serve.build[1] {
            Step::Workdir(workdir) => assert_eq!(workdir.path, build_path),
            other => panic!("expected a workdir step, got {other:?}"),
        }
        match &serve.build[2] {
            Step::Copy(copy) => {
                assert_eq!(copy.source, ".");
                assert_eq!(copy.target, ".");
                assert_eq!(
                    copy.ignore.as_deref(),
                    Some(&[".git".to_owned(), "node_modules".to_owned()][..])
                );
            }
            other => panic!("expected a copy step, got {other:?}"),
        }
        match &serve.build[3] {
            Step::Workdir(workdir) => {
                assert_eq!(workdir.path, build_path.join("apps/site"));
            }
            other => panic!("expected a workdir step, got {other:?}"),
        }
        let install_step = serve
            .build
            .iter()
            .find_map(|step| match step {
                Step::Run(run) if run.group.as_deref() == Some("install") => Some(run),
                _ => None,
            })
            .expect("install step");
        assert_eq!(install_step.inputs, None);
    }

    #[test]
    fn test_subdir_uses_app_specific_anybuild_by_default() {
        // An unparseable root Anybuild must not be touched when --subdir
        // selects the app-specific file.
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path();
        write_node_app(workspace, "apps/site");
        write(&workspace.join("Anybuild"), "this is not valid starlark\n");
        generate(workspace, "apps/site");

        let ctx = resolve(workspace, Some("apps/site"), None);

        assert_eq!(
            ctx.anybuild_dir,
            ctx.paths.workspace_root.join(".anybuild").join("apps-site")
        );
        assert!(ctx
            .serve
            .cwd
            .as_deref()
            .is_some_and(|cwd| cwd.ends_with("/app")));
    }

    #[test]
    fn test_subdir_anybuild_dir_uses_app_specific_slug() {
        // The decisive shared assertion of the run/deploy subdir tests: the
        // resolved anybuild dir is `.anybuild/<subdir-slug>` (the fake
        // runner/backend plumbing they exercise is Python-CLI wiring).
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path();
        write_node_app(workspace, "apps/site");

        let paths = resolve_project_paths(workspace, Some("apps/site")).unwrap();
        assert_eq!(
            default_anybuild_dir(&paths),
            paths.workspace_root.join(".anybuild").join("apps-site")
        );
    }

    #[test]
    fn test_subdir_env_files_override_workspace_env() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path();
        let app_path = write_node_app(workspace, "apps/site");
        write(&workspace.join(".env"), "SHARED=root\nROOT_ONLY=root\n");
        write(
            &workspace.join(".env.production"),
            "SHARED=root-prod\nROOT_PROD=root-prod\nGENERIC=workspace-prod\n",
        );
        write(
            &app_path.join(".env"),
            "SHARED=app\nAPP_ONLY=app\nGENERIC=app\n",
        );
        write(
            &app_path.join(".env.production"),
            "SHARED=app-prod\nAPP_PROD=app-prod\n",
        );
        let paths = resolve_project_paths(workspace, Some("apps/site")).unwrap();
        let mut env: IndexMap<String, String> = IndexMap::new();

        load_env_files(&paths, Some("production"), &mut env);

        assert_eq!(env["SHARED"], "app-prod");
        assert_eq!(env["GENERIC"], "app");
        assert_eq!(env["ROOT_ONLY"], "root");
        assert_eq!(env["ROOT_PROD"], "root-prod");
        assert_eq!(env["APP_ONLY"], "app");
        assert_eq!(env["APP_PROD"], "app-prod");
    }

    #[test]
    fn test_subdir_validation_rejects_invalid_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path();
        write_node_app(workspace, "apps/site");

        for subdir in ["/tmp/app", "../outside", "missing"] {
            assert!(
                resolve_project_paths(workspace, Some(subdir)).is_err(),
                "subdir {subdir:?} should be rejected"
            );
        }
    }
}
