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
    default_shipit_dir, get_shipit_path, read_shipit_subdir, resolve_project_paths,
    ProjectPaths,
};

/// The bundled assets dir (`src/shipit/assets`), the Python `ASSETS_PATH`.
/// Overridable with SHIPIT_ASSETS while the two implementations coexist.
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

pub fn base_config_for(
    app_path: &Path,
    overrides: &CommandOverrides,
) -> BaseConfig {
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
    let provider = load_provider(
        &paths.app_path,
        &base,
        overrides.use_provider.as_deref(),
    )?;
    let mut config = load_provider_config(provider, &paths.app_path, base)?;
    if let Some(patch) = &overrides.config {
        let patch: serde_json::Value = serde_json::from_str(patch)
            .map_err(|e| anyhow!("--config must be valid JSON: {e}"))?;
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
