//! The shared pipeline behind the CLI commands (port of
//! `resolve_project_context` / `resolve_environment`).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anybuild_build::docker::DockerBuildBackend;
use anybuild_build::local::LocalBuildBackend;
use anybuild_build::BuildBackend;
use anybuild_plan::layout::{MountLayout, WasmerServeLayout};
use anybuild_plan::Serve;
use anybuild_providers::{
    base::BaseConfig, load_provider, load_provider_config, workspace, ProviderConfig,
};
use anybuild_run::local::LocalRunner;
use anybuild_run::wasmer::WasmerRunner;
use anybuild_run::Runner;
use anybuild_starlark::eval::{evaluate_anybuild, EvaluateOptions};
use anybuild_starlark::loader::StdlibSource;
use anyhow::{anyhow, Result};

use crate::internal::paths::{
    default_anybuild_dir, get_anybuild_path, read_anybuild_subdir, resolve_project_paths,
    ProjectPaths,
};
use crate::internal::resources;
use crate::sdk::CommandOverrides;

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
    pub anybuild_dir: PathBuf,
    pub runtime_resources: resources::RuntimeResources,
    pub build_backend: Rc<RefCell<dyn BuildBackend>>,
    pub runner: Box<dyn Runner>,
}

pub fn resolve_environment(
    paths: &ProjectPaths,
    options: &EnvironmentOptions,
) -> Result<Environment> {
    migrate_legacy_state_dir(paths)?;
    let anybuild_dir = default_anybuild_dir(paths);
    let runtime_resources = resources::resolve()?;
    let build_backend: Rc<RefCell<dyn BuildBackend>> =
        if options.docker || options.docker_client.is_some() {
            Rc::new(RefCell::new(DockerBuildBackend::new(
                paths.workspace_root.clone(),
                runtime_resources.assets_dir.clone(),
                options.docker_client.clone(),
                options.docker_opts.clone(),
                Some(anybuild_dir.clone()),
            )?))
        } else {
            Rc::new(RefCell::new(LocalBuildBackend::new(
                paths.workspace_root.clone(),
                runtime_resources.assets_dir.clone(),
                Some(anybuild_dir.clone()),
            )))
        };
    let runner: Box<dyn Runner> = if options.wasmer {
        Box::new(WasmerRunner::new(
            build_backend.clone(),
            paths.workspace_root.clone(),
            options.wasmer_registry.clone(),
            options.wasmer_token.clone(),
            options.wasmer_bin.clone(),
            Some(anybuild_dir.clone()),
        ))
    } else {
        Box::new(LocalRunner::new(
            build_backend.clone(),
            paths.workspace_root.clone(),
            Some(anybuild_dir.clone()),
        ))
    };
    Ok(Environment {
        anybuild_dir,
        runtime_resources,
        build_backend,
        runner,
    })
}

fn migrate_legacy_state_dir(paths: &ProjectPaths) -> Result<()> {
    let current = paths.workspace_root.join(".anybuild");
    let legacy = paths.workspace_root.join(".shipit");
    if current.exists() || !legacy.exists() {
        return Ok(());
    }
    std::fs::rename(&legacy, &current).map_err(|err| {
        anyhow!(
            "Could not rename legacy {} directory to {}: {err}",
            legacy.display(),
            current.display()
        )
    })?;
    crate::event::emit(crate::Event::LegacyRenamed {
        from: legacy,
        to: current,
    });
    Ok(())
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

pub struct ProjectContext {
    pub paths: ProjectPaths,
    pub anybuild_dir: PathBuf,
    pub provider: &'static str,
    pub provider_config: ProviderConfig,
    pub serve: Serve,
    _runtime_resources: resources::RuntimeResources,
    pub build_backend: Rc<RefCell<dyn BuildBackend>>,
    pub runner: Box<dyn Runner>,
}

pub fn base_config_for(app_path: &Path, overrides: &CommandOverrides) -> Result<BaseConfig> {
    let mut base = BaseConfig::from_env()?;
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
        if let Some(env_port) = anybuild_common::event::environment_var("PORT") {
            if !env_port.is_empty() && env_port.chars().all(|c| c.is_ascii_digit()) {
                serve_port = env_port.parse().ok();
            }
        }
    }
    if let Some(port) = serve_port {
        base.port = Some(port);
    }
    Ok(base)
}

/// Load provider + config for a resolved app path (detection half of
/// `resolve_project_context`).
pub fn load_project_config(
    paths: &ProjectPaths,
    overrides: &CommandOverrides,
) -> Result<(&'static str, ProviderConfig)> {
    let base = base_config_for(&paths.app_path, overrides)?;
    let provider = load_provider(&paths.app_path, &base, overrides.use_provider.as_deref())?;
    let mut config = load_provider_config(provider, &paths.app_path, base)?;
    if let Some(patch) = &overrides.config {
        config = anybuild_providers::merge_config_json(provider, &config, patch)?;
    }
    workspace::apply_subdir_provider_config(&mut config, paths.subdir.as_deref());
    workspace::apply_subdir_workspace_config(&paths.workspace_root, &mut config);
    Ok((provider, config))
}

/// Resolve paths (including the subdir recorded in the Anybuild file), load
/// the config, and evaluate the Anybuild file into a plan.
pub fn resolve_project_context(
    path: &Path,
    subdir: Option<&str>,
    anybuild_path: Option<&Path>,
    overrides: &CommandOverrides,
    env_options: &EnvironmentOptions,
) -> Result<ProjectContext> {
    let mut paths = resolve_project_paths(path, subdir)?;
    let anybuild_file = get_anybuild_path(&paths, anybuild_path)?;
    if paths.subdir.is_none() {
        if let Some(marker) = read_anybuild_subdir(&anybuild_file) {
            paths = resolve_project_paths(&paths.workspace_root.clone(), Some(&marker))?;
        }
    }
    let environment = resolve_environment(&paths, env_options)?;
    let Environment {
        anybuild_dir,
        runtime_resources,
        build_backend,
        mut runner,
    } = environment;

    let (provider, provider_config) = load_project_config(&paths, overrides)?;
    // Apply the runner's config hook before evaluation (identity for the
    // local runner; the Wasmer runner selects its runtime dependencies and
    // preparation behavior through config overrides).
    let provider_config = runner.prepare_config(provider_config);

    let serve = evaluate_anybuild(EvaluateOptions {
        anybuild_file,
        project_root: Some(paths.workspace_root.clone()),
        config: provider_config.to_json(),
        layout: Box::new(EnvironmentLayout {
            backend: build_backend.clone(),
            wasmer: env_options.wasmer,
        }),
        stdlib: StdlibSource::Dir(runtime_resources.starlib_dir.clone()),
    })?;

    Ok(ProjectContext {
        paths,
        anybuild_dir,
        provider,
        provider_config,
        serve,
        _runtime_resources: runtime_resources,
        build_backend,
        runner,
    })
}
