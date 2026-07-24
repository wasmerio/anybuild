//! Evaluate an Anybuild file into a Serve plan.
//!
//! Port of `evaluator.py::evaluate_anybuild`. The provider config arrives as
//! its JSON form (`model_dump(mode="json")` with sets pre-sorted) — the
//! same value the `config` global exposes to Starlark.

use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};
use starlark::environment::{Globals, GlobalsBuilder, LibraryExtension};
use starlark::values::none::NoneType;

use crate::plan::layout::MountLayout;
use crate::plan::{RunStep, Serve, Step};

use crate::starlark::config::ConfigValue;
use crate::starlark::ctx::{anybuild_builtins, Ctx};
use crate::starlark::loader::{ModuleGraph, StdlibSource};

const LIBRARY_EXTENSIONS: &[LibraryExtension] = &[
    LibraryExtension::StructType,
    LibraryExtension::Json,
    LibraryExtension::Print,
    LibraryExtension::Pprint,
    LibraryExtension::Partial,
    LibraryExtension::Map,
    LibraryExtension::Filter,
    LibraryExtension::Debug,
];

pub struct EvaluateOptions {
    pub anybuild_file: PathBuf,
    pub project_root: Option<PathBuf>,
    /// Provider config as JSON (objects become attr-accessible namespaces,
    /// sets are pre-sorted lists).
    pub config: serde_json::Value,
    pub layout: Box<dyn MountLayout>,
    pub stdlib: StdlibSource,
}

fn config_str(config: &serde_json::Value, path: &[&str]) -> Option<String> {
    let mut current = config;
    for key in path {
        current = current.get(key)?;
    }
    current.as_str().map(str::to_owned)
}

fn port_string(config: &serde_json::Value) -> String {
    match config.get("port") {
        Some(serde_json::Value::Number(n)) => n.to_string(),
        Some(serde_json::Value::String(s)) if !s.is_empty() => s.clone(),
        _ => "8080".to_owned(),
    }
}

fn globals(port: &str, config: Option<&serde_json::Map<String, serde_json::Value>>) -> Globals {
    let mut builder = GlobalsBuilder::extended_by(LIBRARY_EXTENSIONS).with(anybuild_builtins);
    builder.set("PORT", port.to_owned());
    if let Some(map) = config {
        builder.set("config", ConfigValue(map.clone()));
    }
    builder.build()
}

pub fn evaluate_anybuild(options: EvaluateOptions) -> Result<Serve> {
    let EvaluateOptions {
        anybuild_file,
        project_root,
        config,
        layout,
        stdlib,
    } = options;

    let source = std::fs::read_to_string(&anybuild_file)
        .with_context(|| format!("reading {}", anybuild_file.display()))?;
    let workspace_root = match project_root {
        Some(root) => root,
        None => anybuild_file
            .canonicalize()
            .unwrap_or_else(|_| anybuild_file.clone())
            .parent()
            .ok_or_else(|| anyhow!("Anybuild file has no parent directory"))?
            .to_path_buf(),
    };
    // file_exists() probes the app directory: the subdir for subdir projects.
    let mut source_dir = workspace_root.clone();
    if let Some(app_subdir) = config_str(&config, &["app_subdir"]) {
        if !app_subdir.is_empty() {
            source_dir = workspace_root.join(app_subdir);
        }
    }

    let config_map = config
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow!("provider config must be a JSON object"))?;
    let port = port_string(&config);

    let ctx = Ctx::new(layout, Some(source_dir));

    // Library modules get the builtins but not `config`: provider libraries
    // receive the config as a function parameter, keeping them pure.
    let lib_globals = globals(&port, None);
    let entry_globals = globals(&port, Some(&config_map));

    let had_loads = {
        let mut graph = ModuleGraph::new(workspace_root.clone(), stdlib, lib_globals, &ctx);
        graph.eval_entry(source, &anybuild_file, "Anybuild", &entry_globals)?
    };

    let mut serves = ctx.serves.into_inner();
    if serves.is_empty() {
        bail!("No serve definition found in {}", anybuild_file.display());
    }
    if serves.len() > 1 {
        bail!("Only one serve is allowed for now");
    }
    let (_, mut serve) = serves.pop().expect("one serve");

    if !had_loads {
        // Legacy fully-inlined Anybuild files predate the Starlark stdlib's
        // override pass, so the host re-applies the user's command
        // overrides. Stdlib (load()-based) files already did this in
        // build_and_serve — replaying would clobber user edits.
        apply_command_overrides(&mut serve, &config, &port);
    }

    Ok(serve)
}

/// Port of the legacy post-eval override block (evaluator.py).
fn apply_command_overrides(serve: &mut Serve, config: &serde_json::Value, port: &str) {
    let start = config_str(config, &["commands", "start"]);
    let after_deploy = config_str(config, &["commands", "after_deploy"]);
    let build_cmd = config_str(config, &["commands", "build"]);
    let install_cmd = config_str(config, &["commands", "install"]);

    if let Some(start) = &start {
        serve.commands.insert("start".to_owned(), start.clone());
    }
    if let Some(after_deploy) = &after_deploy {
        serve
            .commands
            .insert("after_deploy".to_owned(), after_deploy.clone());
    }

    if build_cmd.is_some() || install_cmd.is_some() {
        let mut new_build: Vec<Step> = Vec::with_capacity(serve.build.len());
        let mut has_done_build = false;
        let mut has_done_install = false;
        for step in serve.build.drain(..) {
            match &step {
                Step::Run(run) => {
                    let group = run.group.as_deref();
                    if group == Some("build") && !has_done_build && build_cmd.is_some() {
                        new_build.push(Step::Run(RunStep {
                            command: build_cmd.clone().unwrap(),
                            inputs: None,
                            outputs: None,
                            group: Some("build".to_owned()),
                        }));
                        has_done_build = true;
                    } else if group == Some("install") && !has_done_install && install_cmd.is_some()
                    {
                        new_build.push(Step::Run(RunStep {
                            command: install_cmd.clone().unwrap(),
                            inputs: None,
                            outputs: None,
                            group: Some("install".to_owned()),
                        }));
                        has_done_install = true;
                    } else {
                        new_build.push(step);
                    }
                }
                _ => new_build.push(step),
            }
        }
        if !has_done_install {
            if let Some(install) = &install_cmd {
                new_build.push(Step::Run(RunStep {
                    command: install.clone(),
                    inputs: None,
                    outputs: None,
                    group: Some("install".to_owned()),
                }));
            }
        }
        if !has_done_build {
            if let Some(build) = &build_cmd {
                new_build.push(Step::Run(RunStep {
                    command: build.clone(),
                    inputs: None,
                    outputs: None,
                    group: Some("build".to_owned()),
                }));
            }
        }
        serve.build = new_build;
    }

    for key in ["start", "after_deploy"] {
        if let Some(command) = serve.commands.get(key) {
            if !command.is_empty() {
                let replaced = command.replace("$PORT", port);
                serve.commands.insert(key.to_owned(), replaced);
            }
        }
    }
}

/// Keep NoneType referenced so builtins returning it stay obvious.
#[allow(dead_code)]
fn _unused() -> NoneType {
    NoneType
}
