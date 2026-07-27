//! Starlark module loading and the load graph.
//!
//! Port of `starlark_loader.py`. Modules are evaluated bottom-up, frozen,
//! and served through a per-module `ReturnFileLoader` keyed by the original
//! label strings. The shared cache and cycle detection are keyed by
//! resolved path: relative labels resolve per loading file, so the same
//! label string can name different modules.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use starlark::environment::{FrozenModule, Globals, Module};
use starlark::eval::{Evaluator, ReturnFileLoader};
use starlark::syntax::{AstModule, Dialect, DialectTypes};

use crate::starlark::ctx::Ctx;

pub const STARLIB_NAMESPACE: &str = "anybuild";
pub const LEGACY_STARLIB_NAMESPACE: &str = "shipit";

/// Strict superset of the previous dialect: legacy fully-inlined Anybuild
/// files keep evaluating unchanged.
pub fn dialect() -> Dialect {
    Dialect {
        enable_def: true,
        enable_lambda: true,
        enable_load: true,
        enable_keyword_only_arguments: true,
        enable_positional_only_arguments: false,
        enable_types: DialectTypes::ParseOnly,
        enable_load_reexport: true,
        enable_top_level_stmt: true,
        enable_f_strings: true,
        _non_exhaustive: (),
    }
}

/// Where `//anybuild/...` labels resolve. `Dir` reads from a directory on
/// disk; the embedded variant lands with the CLI crate.
#[derive(Debug, Clone)]
pub enum StdlibSource {
    Dir(PathBuf),
}

impl StdlibSource {
    fn resolve(&self, rel: &str, fname: &str) -> PathBuf {
        match self {
            StdlibSource::Dir(root) => {
                if rel.is_empty() {
                    root.join(fname)
                } else {
                    root.join(rel).join(fname)
                }
            }
        }
    }
}

pub fn resolve_label(
    label: &str,
    loading_file: &Path,
    project_root: &Path,
    stdlib: &StdlibSource,
) -> Result<PathBuf> {
    if let Some(rest) = label.strip_prefix("//") {
        let (pkg, fname) = rest
            .split_once(':')
            .filter(|(_, fname)| !fname.is_empty())
            .ok_or_else(|| anyhow!("Invalid load label {label:?}: expected //package:file.bzl"))?;
        for namespace in [STARLIB_NAMESPACE, LEGACY_STARLIB_NAMESPACE] {
            if pkg == namespace || pkg.starts_with(&format!("{namespace}/")) {
                let rel = pkg[namespace.len()..].trim_matches('/');
                let fname = modern_module_name(fname);
                return Ok(stdlib.resolve(rel, &fname));
            }
        }
        return Ok(prefer_modern_module(project_root.join(pkg).join(fname)));
    }
    let parent = loading_file
        .parent()
        .ok_or_else(|| anyhow!("loading file has no parent directory"))?;
    Ok(normalize(&prefer_modern_module(parent.join(label))))
}

fn modern_module_name(name: &str) -> String {
    name.strip_suffix(".shipit")
        .map_or_else(|| name.to_owned(), |stem| format!("{stem}.bzl"))
}

fn prefer_modern_module(path: PathBuf) -> PathBuf {
    if path.is_file() {
        return path;
    }
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return path;
    };
    let modern = path.with_file_name(modern_module_name(name));
    if modern.is_file() {
        modern
    } else {
        path
    }
}

/// Lexical path normalization (Python's `Path.resolve()` also follows
/// symlinks; for cache keys we canonicalize when the file exists).
fn normalize(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub struct ModuleGraph<'c> {
    pub project_root: PathBuf,
    pub stdlib: StdlibSource,
    pub lib_globals: Globals,
    pub ctx: &'c Ctx,
    /// Keyed by resolved path, not label.
    frozen: HashMap<PathBuf, FrozenModule>,
}

impl<'c> ModuleGraph<'c> {
    pub fn new(
        project_root: PathBuf,
        stdlib: StdlibSource,
        lib_globals: Globals,
        ctx: &'c Ctx,
    ) -> Self {
        Self {
            project_root,
            stdlib,
            lib_globals,
            ctx,
            frozen: HashMap::new(),
        }
    }

    /// Load the dependencies of one parsed module, returning them keyed by
    /// the literal label each `load()` statement used.
    fn deps_for(
        &mut self,
        ast: &AstModule,
        path: &Path,
        stack: &mut Vec<(PathBuf, String)>,
    ) -> Result<Vec<(String, FrozenModule)>> {
        let mut deps = Vec::new();
        for load in ast.loads() {
            let label = load.module_id.to_owned();
            let dep_path = resolve_label(&label, path, &self.project_root, &self.stdlib)?;
            let module = self.load_module(&label, &dep_path, stack)?;
            deps.push((label, module));
        }
        Ok(deps)
    }

    fn load_module(
        &mut self,
        label: &str,
        path: &Path,
        stack: &mut Vec<(PathBuf, String)>,
    ) -> Result<FrozenModule> {
        let key = normalize(path);
        if let Some(module) = self.frozen.get(&key) {
            return Ok(module.clone());
        }
        if stack.iter().any(|(seen, _)| *seen == key) {
            let labels: Vec<&str> = stack
                .iter()
                .map(|(_, label)| label.as_str())
                .chain([label])
                .collect();
            bail!("load() cycle: {}", labels.join(" -> "));
        }
        if !path.is_file() {
            bail!("load({label:?}): no module at {}", path.display());
        }
        let source =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let ast = AstModule::parse(label, source, &dialect()).map_err(|e| anyhow!("{e}"))?;
        stack.push((key.clone(), label.to_owned()));
        let deps = self.deps_for(&ast, path, stack)?;
        stack.pop();
        let frozen = eval_frozen(ast, &deps, &self.lib_globals, self.ctx)?;
        self.frozen.insert(key, frozen.clone());
        Ok(frozen)
    }

    /// Evaluate the entry file with its load() graph resolved. Returns
    /// whether the entry had any load() statements (legacy detection).
    pub fn eval_entry(
        &mut self,
        source: String,
        entry_path: &Path,
        entry_name: &str,
        entry_globals: &Globals,
    ) -> Result<bool> {
        let ast = AstModule::parse(entry_name, source, &dialect()).map_err(|e| anyhow!("{e}"))?;
        let mut stack = Vec::new();
        let deps = self.deps_for(&ast, entry_path, &mut stack)?;
        let had_loads = !deps.is_empty();
        let ctx = self.ctx;
        Module::with_temp_heap(|module| -> Result<()> {
            let modules: HashMap<&str, &FrozenModule> = deps
                .iter()
                .map(|(label, module)| (label.as_str(), module))
                .collect();
            let loader = ReturnFileLoader { modules: &modules };
            let mut eval = Evaluator::new(&module);
            // Catch statically-detectable errors (e.g. wrong arity in
            // branches this evaluation never executes) at load time.
            eval.enable_static_typechecking(true);
            if !modules.is_empty() {
                eval.set_loader(&loader);
            }
            eval.extra = Some(ctx);
            eval.eval_module(ast, entry_globals)
                .map_err(|e| anyhow!("{e}"))?;
            Ok(())
        })?;
        Ok(had_loads)
    }
}

fn eval_frozen(
    ast: AstModule,
    deps: &[(String, FrozenModule)],
    lib_globals: &Globals,
    ctx: &Ctx,
) -> Result<FrozenModule> {
    Module::with_temp_heap(|module| -> Result<FrozenModule> {
        {
            let modules: HashMap<&str, &FrozenModule> = deps
                .iter()
                .map(|(label, module)| (label.as_str(), module))
                .collect();
            let loader = ReturnFileLoader { modules: &modules };
            let mut eval = Evaluator::new(&module);
            eval.enable_static_typechecking(true);
            if !modules.is_empty() {
                eval.set_loader(&loader);
            }
            eval.extra = Some(ctx);
            eval.eval_module(ast, lib_globals)
                .map_err(|e| anyhow!("{e}"))?;
        }
        module.freeze().map_err(|e| anyhow!("{:?}", e))
    })
}
