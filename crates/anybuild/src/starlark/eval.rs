//! Evaluate an Anybuild file into a resolved provider config and Serve plan.

use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};
use starlark::environment::{Globals, GlobalsBuilder, LibraryExtension};

use crate::plan::layout::MountLayout;
use crate::plan::Serve;
use crate::providers::ProviderConfig;
use crate::starlark::config::{ConfigResolutionOptions, PersistedConfig};
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
    pub project_root: PathBuf,
    pub source_dir: PathBuf,
    pub config_resolution: ConfigResolutionOptions,
    pub layout: Box<dyn MountLayout>,
    pub stdlib: StdlibSource,
}

pub struct EvaluatedAnybuild {
    pub serve: Serve,
    pub provider_config: ProviderConfig,
    pub persisted: PersistedConfig,
}

fn globals() -> Globals {
    GlobalsBuilder::extended_by(LIBRARY_EXTENSIONS)
        .with(anybuild_builtins)
        .build()
}

pub fn evaluate_anybuild(options: EvaluateOptions) -> Result<EvaluatedAnybuild> {
    let EvaluateOptions {
        anybuild_file,
        project_root,
        source_dir,
        config_resolution,
        layout,
        stdlib,
    } = options;

    let source = std::fs::read_to_string(&anybuild_file)
        .with_context(|| format!("reading {}", anybuild_file.display()))?;
    let ctx = Ctx::new(layout, Some(source_dir)).with_config_resolution(config_resolution);
    let lib_globals = globals();
    let entry_globals = globals();

    let evaluation = {
        let mut graph = ModuleGraph::new(project_root, stdlib, lib_globals, &ctx);
        graph.eval_entry(source, &anybuild_file, "Anybuild", &entry_globals)
    };
    if let Err(error) = evaluation {
        if ctx.resolved_config.borrow().is_none() {
            return Err(error.context(
                "The Anybuild file must construct its provider config. Run `anybuild generate` to update it",
            ));
        }
        return Err(error);
    }

    let resolved = ctx.resolved_config.borrow_mut().take().ok_or_else(|| {
        anyhow!(
            "The Anybuild file did not construct a provider config. Run `anybuild generate` to update it"
        )
    })?;
    let mut serves = ctx.serves.into_inner();
    if serves.is_empty() {
        bail!("No serve definition found in {}", anybuild_file.display());
    }
    if serves.len() > 1 {
        bail!("Only one serve is allowed for now");
    }
    let (_, serve) = serves.pop().expect("one serve");

    Ok(EvaluatedAnybuild {
        serve,
        provider_config: resolved.effective,
        persisted: resolved.persisted,
    })
}
