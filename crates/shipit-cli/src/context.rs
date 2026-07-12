//! The shared pipeline behind the CLI commands (port of
//! `resolve_project_context` / `resolve_environment`).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::{anyhow, Result};
use shipit_build::docker::DockerBuildBackend;
use shipit_build::local::LocalBuildBackend;
use shipit_build::BuildBackend;
use shipit_plan::layout::{MountLayout, WasmerServeLayout};
use shipit_plan::Serve;
use shipit_providers::{
    base::BaseConfig, load_provider, load_provider_config, workspace, ProviderConfig,
};
use shipit_run::local::LocalRunner;
use shipit_run::wasmer::WasmerRunner;
use shipit_run::Runner;
use shipit_starlark::eval::{evaluate_shipit, EvaluateOptions};
use shipit_starlark::loader::StdlibSource;

use crate::generator::starlib_dir;
use crate::paths::{
    default_shipit_dir, get_shipit_path, read_shipit_subdir, resolve_project_paths, ProjectPaths,
};

/// The bundled assets dir (shared with the legacy Python tree).
/// Overridable with SHIPIT_ASSETS.
pub fn assets_dir() -> PathBuf {
    if let Ok(path) = std::env::var("SHIPIT_ASSETS") {
        return PathBuf::from(path);
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../src/shipit/assets")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("src/shipit/assets"))
}

/// Which backend/runner pair to resolve (the wasmer/docker flag surface
/// shared by auto/build/run/plan/deploy).
#[derive(Debug, Clone, Default)]
pub struct EnvironmentOptions {
    pub wasmer: bool,
    pub wasmer_bin: Option<String>,
    pub wasmer_registry: Option<String>,
    pub wasmer_token: Option<String>,
    pub docker: bool,
    pub docker_client: Option<String>,
    pub docker_opts: Option<String>,
}

/// Backend + runner for an already-resolved project (port of
/// `resolve_environment`).
pub struct Environment {
    pub shipit_dir: PathBuf,
    pub build_backend: Rc<RefCell<dyn BuildBackend>>,
    pub runner: Box<dyn Runner>,
}

pub fn resolve_environment(
    paths: &ProjectPaths,
    options: &EnvironmentOptions,
) -> Result<Environment> {
    let shipit_dir = default_shipit_dir(paths);
    let build_backend: Rc<RefCell<dyn BuildBackend>> =
        if options.docker || options.docker_client.is_some() {
            Rc::new(RefCell::new(DockerBuildBackend::new(
                paths.workspace_root.clone(),
                assets_dir(),
                options.docker_client.clone(),
                options.docker_opts.clone(),
                Some(shipit_dir.clone()),
            )?))
        } else {
            Rc::new(RefCell::new(LocalBuildBackend::new(
                paths.workspace_root.clone(),
                assets_dir(),
                Some(shipit_dir.clone()),
            )))
        };
    let runner: Box<dyn Runner> = if options.wasmer {
        Box::new(WasmerRunner::new(
            build_backend.clone(),
            paths.workspace_root.clone(),
            options.wasmer_registry.clone(),
            options.wasmer_token.clone(),
            options.wasmer_bin.clone(),
            Some(shipit_dir.clone()),
        ))
    } else {
        Box::new(LocalRunner::new(
            build_backend.clone(),
            paths.workspace_root.clone(),
            Some(shipit_dir.clone()),
        ))
    };
    Ok(Environment {
        shipit_dir,
        build_backend,
        runner,
    })
}

/// Mount layout for evaluation: build/volume paths come from the resolved
/// backend, serve paths from the resolved runner (Python's Ctx asks the
/// live backend/runner pair; a fixed layout would break --wasmer/--docker).
struct EnvironmentLayout {
    backend: Rc<RefCell<dyn BuildBackend>>,
    wasmer: bool,
}

impl MountLayout for EnvironmentLayout {
    fn build_mount_path(&self, name: &str) -> PathBuf {
        self.backend.borrow().get_build_mount_path(name)
    }

    fn serve_mount_path(&self, name: &str) -> PathBuf {
        if self.wasmer {
            WasmerServeLayout::serve_mount_path(name)
        } else {
            self.backend.borrow().get_artifact_mount_path(name)
        }
    }

    fn volume_path(&self, name: &str) -> PathBuf {
        self.backend.borrow().get_volume_path(name)
    }
}

pub struct CommandOverrides {
    pub start_command: Option<String>,
    pub install_command: Option<String>,
    pub build_command: Option<String>,
    pub serve_port: Option<i64>,
    pub use_provider: Option<String>,
    pub config: Option<String>,
}

pub struct ProjectContext {
    pub paths: ProjectPaths,
    pub shipit_dir: PathBuf,
    pub provider: &'static str,
    pub provider_config: ProviderConfig,
    pub serve: Serve,
    pub build_backend: Rc<RefCell<dyn BuildBackend>>,
    pub runner: Box<dyn Runner>,
}

pub fn base_config_for(app_path: &Path, overrides: &CommandOverrides) -> BaseConfig {
    let mut base = BaseConfig::default();
    base.commands.enrich_from_path(app_path);
    if let Some(start) = &overrides.start_command {
        base.commands.start = Some(start.clone());
    }
    if let Some(install) = &overrides.install_command {
        base.commands.install = Some(install.clone());
    }
    if let Some(build) = &overrides.build_command {
        base.commands.build = Some(build.clone());
    }
    let mut serve_port = overrides.serve_port;
    if serve_port.is_none() {
        if let Ok(env_port) = std::env::var("PORT") {
            if !env_port.is_empty() && env_port.chars().all(|c| c.is_ascii_digit()) {
                serve_port = env_port.parse().ok();
            }
        }
    }
    if let Some(port) = serve_port {
        base.port = Some(port);
    }
    base
}

/// Load provider + config for a resolved app path (detection half of
/// `resolve_project_context`).
pub fn load_project_config(
    paths: &ProjectPaths,
    overrides: &CommandOverrides,
) -> Result<(&'static str, ProviderConfig)> {
    let base = base_config_for(&paths.app_path, overrides);
    let provider = load_provider(&paths.app_path, &base, overrides.use_provider.as_deref())?;
    let mut config = load_provider_config(provider, &paths.app_path, base)?;
    if let Some(patch) = &overrides.config {
        let patch: serde_json::Value =
            serde_json::from_str(patch).map_err(|e| anyhow!("--config must be valid JSON: {e}"))?;
        config = shipit_providers::merge_config_json(provider, &config, &patch)?;
    }
    workspace::apply_subdir_provider_config(&mut config, paths.subdir.as_deref());
    workspace::apply_subdir_workspace_config(&paths.workspace_root, &mut config);
    Ok((provider, config))
}

/// Resolve paths (including the subdir recorded in the Shipit file), load
/// the config, and evaluate the Shipit file into a plan.
pub fn resolve_project_context(
    path: &Path,
    subdir: Option<&str>,
    shipit_path: Option<&Path>,
    overrides: &CommandOverrides,
    env_options: &EnvironmentOptions,
) -> Result<ProjectContext> {
    let mut paths = resolve_project_paths(path, subdir)?;
    let shipit_file = get_shipit_path(&paths, shipit_path)?;
    if paths.subdir.is_none() {
        if let Some(marker) = read_shipit_subdir(&shipit_file) {
            paths = resolve_project_paths(&paths.workspace_root.clone(), Some(&marker))?;
        }
    }
    let environment = resolve_environment(&paths, env_options)?;
    let Environment {
        shipit_dir,
        build_backend,
        mut runner,
    } = environment;

    let (provider, provider_config) = load_project_config(&paths, overrides)?;
    // Apply the runner's config hook before evaluation (identity for the
    // local runner; the wasmer runner's plan-visible/runner-only split is
    // an invariant — see runners/wasmer semantics in plans/rust-migration.md).
    let provider_config = runner.prepare_config(provider_config);

    let serve = evaluate_shipit(EvaluateOptions {
        shipit_file,
        project_root: Some(paths.workspace_root.clone()),
        config: provider_config.to_json(),
        layout: Box::new(EnvironmentLayout {
            backend: build_backend.clone(),
            wasmer: env_options.wasmer,
        }),
        stdlib: StdlibSource::Dir(starlib_dir()),
    })?;

    Ok(ProjectContext {
        paths,
        shipit_dir,
        provider,
        provider_config,
        serve,
        build_backend,
        runner,
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use shipit_plan::layout::LocalLayout;
    use shipit_plan::{RunStep, Serve, Step};
    use shipit_providers::{load_provider, load_provider_config, merge_config_json, BaseConfig};
    use shipit_starlark::eval::{evaluate_shipit, EvaluateOptions};
    use shipit_starlark::loader::StdlibSource;

    use super::*;
    use crate::generator::{entrypoint, generate_shipit_loader, starlib_dir};

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

    fn group_commands<'a>(serve: &'a Serve, group: &str) -> Vec<&'a str> {
        serve
            .build
            .iter()
            .filter_map(|step| match step {
                Step::Run(run) if run.group.as_deref() == Some(group) => Some(run.command.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Port of tests/test_project_context.py::
    /// test_out_of_tree_shipit_path_probes_the_workspace — file_exists()
    /// must target the workspace even when the Shipit file lives elsewhere
    /// (--shipit-path, --temp-shipit).
    #[test]
    fn out_of_tree_shipit_path_probes_the_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("app");
        std::fs::create_dir(&workspace).unwrap();
        std::fs::write(
            workspace.join("package.json"),
            r#"{"name": "app", "scripts": {"start": "node index.js"}}"#,
        )
        .unwrap();
        std::fs::write(workspace.join("package-lock.json"), "{}").unwrap();
        std::fs::write(workspace.join("index.js"), "console.log('hi')\n").unwrap();

        let elsewhere = tmp.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        let shipit_file = elsewhere.join("Shipit");
        std::fs::write(
            &shipit_file,
            generate_shipit_loader(&entrypoint("node").unwrap(), None, None),
        )
        .unwrap();

        let context = resolve_project_context(
            &workspace,
            None,
            Some(&shipit_file),
            &no_overrides(),
            &EnvironmentOptions::default(),
        )
        .unwrap();

        let commands = run_commands(&context.serve);
        assert!(
            commands.iter().any(|c| c.contains("npm install")),
            "{commands:?}"
        );
    }

    /// Port of tests/plan_helpers.py::evaluate_project_plan — detect the
    /// provider, load its config (with the JSON patch, the `--config`
    /// merge), generate the two-line loader Shipit, and evaluate it
    /// through the Starlark stdlib.
    fn evaluate_project_plan(
        workspace: &Path,
        tmp: &Path,
        config_patch: serde_json::Value,
    ) -> Serve {
        let mut base = BaseConfig::default();
        base.commands.enrich_from_path(workspace);
        let provider = load_provider(workspace, &base, None).unwrap();
        let config = load_provider_config(provider, workspace, base).unwrap();
        let config = merge_config_json(provider, &config, &config_patch).unwrap();
        let shipit_file = tmp.join("Shipit.plan");
        std::fs::write(
            &shipit_file,
            generate_shipit_loader(&entrypoint(provider).unwrap(), None, None),
        )
        .unwrap();
        evaluate_shipit(EvaluateOptions {
            shipit_file,
            project_root: Some(workspace.to_path_buf()),
            config: config.to_json(),
            layout: Box::new(LocalLayout::new(tmp.join(".shipit"))),
            stdlib: StdlibSource::Dir(starlib_dir()),
        })
        .unwrap()
    }

    /// Port of tests/test_command_overrides.py::
    /// test_build_and_install_overrides_replace_group_steps.
    #[test]
    fn build_and_install_overrides_replace_group_steps() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("app");
        std::fs::create_dir(&workspace).unwrap();
        std::fs::write(workspace.join("requirements.txt"), "flask==3.0.0\n").unwrap();
        std::fs::write(workspace.join("main.py"), "print('ok')\n").unwrap();

        let serve = evaluate_project_plan(
            &workspace,
            tmp.path(),
            serde_json::json!({
                "commands": {
                    "install": "make install",
                    "build": "make assets",
                    "start": "make serve --port $PORT",
                }
            }),
        );

        // The FIRST step of each group is replaced (later same-group steps
        // are kept — long-standing CLI semantics) and nothing is applied
        // twice.
        assert_eq!(
            group_commands(&serve, "install"),
            ["make install", "uv add -r requirements.txt uvicorn"]
        );
        assert_eq!(group_commands(&serve, "build"), ["make assets"]);
        // start override wins and $PORT is substituted.
        assert_eq!(
            serve.commands.get("start").unwrap(),
            "make serve --port 8080"
        );
    }

    /// Port of tests/test_command_overrides.py::
    /// test_build_override_appends_when_no_group_step_exists.
    #[test]
    fn build_override_appends_when_no_group_step_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("site");
        let public = workspace.join("public");
        std::fs::create_dir_all(&public).unwrap();
        std::fs::write(public.join("index.html"), "<h1>ok</h1>\n").unwrap();

        let serve = evaluate_project_plan(
            &workspace,
            tmp.path(),
            serde_json::json!({"commands": {"build": "make site"}}),
        );

        // Static builds have no build-group step; the override is appended
        // once, as the last build step.
        assert_eq!(group_commands(&serve, "build"), ["make site"]);
        assert_eq!(
            serve.build.last().unwrap(),
            &Step::Run(RunStep {
                command: "make site".to_owned(),
                inputs: None,
                outputs: None,
                group: Some("build".to_owned()),
            })
        );
    }
}
