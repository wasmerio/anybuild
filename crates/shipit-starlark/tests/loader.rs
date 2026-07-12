//! Port of tests/test_starlark_loader.py: module-graph loading —
//! relative-label resolution, caching, and cycles.
//!
//! The Python tests read globals off the evaluated entry module; the Rust
//! public API does not expose the entry module's bindings, so each entry
//! surfaces the loaded values through `serve(name = ..., provider = ...)`
//! and the assertions read them back from `Ctx.serves`.

use std::path::Path;

use shipit_plan::layout::LocalLayout;
use shipit_starlark::ctx::{shipit_builtins, Ctx};
use shipit_starlark::loader::{ModuleGraph, StdlibSource};
use starlark::environment::{Globals, GlobalsBuilder};

fn globals() -> Globals {
    GlobalsBuilder::standard().with(shipit_builtins).build()
}

fn write_tree(root: &Path, files: &[(&str, &str)]) {
    for (rel, text) in files {
        let target = root.join(rel);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, text).unwrap();
    }
}

/// Python's `_eval`: evaluate root/Shipit with root as the project root.
/// Returns the registered `(name, provider)` serves.
fn eval(root: &Path) -> anyhow::Result<Vec<(String, String)>> {
    let entry = root.join("Shipit");
    let source = std::fs::read_to_string(&entry)?;
    let ctx = Ctx::new(
        Box::new(LocalLayout::new(root.join(".shipit"))),
        Some(root.to_path_buf()),
    );
    {
        let mut graph = ModuleGraph::new(
            root.to_path_buf(),
            StdlibSource::Dir(root.to_path_buf()),
            globals(),
            &ctx,
        );
        graph.eval_entry(source, &entry, "Shipit", &globals())?;
    }
    Ok(ctx
        .serves
        .into_inner()
        .into_iter()
        .map(|(name, serve)| (name, serve.provider))
        .collect())
}

/// Two files loading the same relative label get their own neighbours.
#[test]
fn same_relative_label_resolves_per_loading_file() {
    let tmp = tempfile::tempdir().unwrap();
    write_tree(
        tmp.path(),
        &[
            (
                "Shipit",
                concat!(
                    "load(\"util.shipit\", \"WHO\")\n",
                    "load(\"lib/helpers.shipit\", \"HELPER_WHO\")\n",
                    // ROOT_WHO / LIB_WHO in the Python test.
                    "serve(name = WHO, provider = HELPER_WHO, build = [], deps = [], commands = {})\n",
                ),
            ),
            ("util.shipit", "WHO = \"root-util\"\n"),
            (
                "lib/helpers.shipit",
                "load(\"util.shipit\", \"WHO\")\nHELPER_WHO = WHO\n",
            ),
            ("lib/util.shipit", "WHO = \"lib-util\"\n"),
        ],
    );

    let serves = eval(tmp.path()).unwrap();
    assert_eq!(
        serves,
        vec![("root-util".to_owned(), "lib-util".to_owned())]
    );
}

/// A repeated label along one load chain is fine when the files differ.
#[test]
fn acyclic_reuse_of_a_label_is_not_a_cycle() {
    let tmp = tempfile::tempdir().unwrap();
    write_tree(
        tmp.path(),
        &[
            (
                "Shipit",
                concat!(
                    "load(\"util.shipit\", \"WHO\")\n",
                    "serve(name = WHO, provider = \"p\", build = [], deps = [], commands = {})\n",
                ),
            ),
            (
                "util.shipit",
                concat!(
                    "load(\"lib/helpers.shipit\", \"HELPER_WHO\")\n",
                    "WHO = \"root:\" + HELPER_WHO\n",
                ),
            ),
            (
                "lib/helpers.shipit",
                "load(\"util.shipit\", \"WHO\")\nHELPER_WHO = WHO\n",
            ),
            ("lib/util.shipit", "WHO = \"lib-util\"\n"),
        ],
    );

    let serves = eval(tmp.path()).unwrap();
    assert_eq!(serves, vec![("root:lib-util".to_owned(), "p".to_owned())]);
}

#[test]
fn true_cycle_is_detected() {
    let tmp = tempfile::tempdir().unwrap();
    write_tree(
        tmp.path(),
        &[
            ("Shipit", "load(\"a.shipit\", \"A\")\nX = A\n"),
            ("a.shipit", "load(\"b.shipit\", \"B\")\nA = B\n"),
            ("b.shipit", "load(\"a.shipit\", \"A\")\nB = A\n"),
        ],
    );

    let err = eval(tmp.path()).unwrap_err();
    assert!(format!("{err:#}").contains("load() cycle"), "{err:#}");
}
