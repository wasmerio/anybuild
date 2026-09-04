//! Evaluator builtin coverage.
//! containment and serve() list handling.

use std::path::Path;

use crate::plan::layout::LocalLayout;
use crate::plan::{RunStep, Service, Step};
use crate::starlark::ctx::{anybuild_builtins, Ctx};
use crate::starlark::loader::{ModuleGraph, StdlibSource};
use starlark::environment::{Globals, GlobalsBuilder, LibraryExtension};

/// Python's `_ctx`: file_exists and serve() never touch the layout
/// (backend/runner in Python), so a throwaway layout is fine.
fn ctx(source_dir: Option<&Path>) -> Ctx {
    Ctx::new(
        Box::new(LocalLayout::new(
            std::env::temp_dir().join("anybuild-test-unused"),
        )),
        source_dir.map(Path::to_path_buf),
    )
}

fn globals() -> Globals {
    GlobalsBuilder::standard().with(anybuild_builtins).build()
}

fn provider_globals() -> Globals {
    GlobalsBuilder::extended_by(&[LibraryExtension::StructType])
        .with(anybuild_builtins)
        .build()
}

/// Evaluate one Anybuild source string against `ctx` (the Python tests call
/// `Ctx.serve` directly; the Rust serve() builtin is only reachable
/// through evaluation).
fn eval_source(ctx: &Ctx, root: &Path, source: &str) -> anyhow::Result<()> {
    let mut graph = ModuleGraph::new(
        root.to_path_buf(),
        StdlibSource::Dir(root.to_path_buf()),
        globals(),
        ctx,
    );
    graph.eval_entry(
        source.to_owned(),
        &root.join("Anybuild"),
        "Anybuild",
        &globals(),
    )?;
    Ok(())
}

#[test]
#[cfg(unix)]
fn file_exists_follows_symlinks_out_of_the_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let shared = tmp.path().join("shared");
    std::fs::create_dir(&shared).unwrap();
    let project = tmp.path().join("project");
    std::fs::create_dir(&project).unwrap();
    std::os::unix::fs::symlink(&shared, project.join("wp-content")).unwrap();

    assert!(ctx(Some(&project)).file_exists("wp-content").unwrap());
}

#[test]
fn file_exists_rejects_traversal() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(tmp.path().join("outside.txt"), "x").unwrap();

    let err = ctx(Some(&project))
        .file_exists("../outside.txt")
        .unwrap_err();
    assert!(err.to_string().contains("escapes"), "{err}");
}

#[test]
fn file_exists_rejects_absolute_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let absolute = tmp.path().join("x");

    let err = ctx(Some(tmp.path()))
        .file_exists(&absolute.to_string_lossy())
        .unwrap_err();
    assert!(err.to_string().contains("project-relative"), "{err}");
}

#[test]
fn serve_drops_none_entries_from_every_list() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = ctx(Some(tmp.path()));
    eval_source(
        &ctx,
        tmp.path(),
        r#"
serve(
    name = "app",
    provider = "test",
    build = [run("echo hi"), None],
    deps = [],
    commands = {"start": "run"},
    services = [service("db", "mysql"), None],
)
"#,
    )
    .unwrap();

    let serves = ctx.serves.borrow();
    let serve = serves.get("app").expect("serve registered");
    assert_eq!(
        serve.build,
        vec![Step::Run(RunStep {
            command: "echo hi".to_owned(),
            inputs: None,
            outputs: None,
            group: None,
        })]
    );
    assert_eq!(
        serve.services,
        Some(vec![Service {
            name: "db".to_owned(),
            provider: "mysql".to_owned(),
        }])
    );
}

#[test]
fn configured_database_services_flow_through_common_serve() {
    let stdlib = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/starlib");

    for provider in ["mysql", "postgres"] {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ctx(Some(tmp.path()));
        let source = format!(
            r#"
load("//anybuild:serve.bzl", "build", "serve")
config = struct(
    name = "app",
    provider = "node",
    port = 8080,
    commands = struct(
        install = None,
        build = None,
        start = None,
        after_deploy = None,
    ),
    services = [struct(name = "database", engine = "{provider}")],
)
serve(
    config,
    build(steps = []),
    commands = {{"start": "node server.js"}},
)
"#
        );
        let mut graph = ModuleGraph::new(
            tmp.path().to_path_buf(),
            StdlibSource::Dir(stdlib.clone()),
            provider_globals(),
            &ctx,
        );
        graph
            .eval_entry(
                source,
                &tmp.path().join("Anybuild"),
                "Anybuild",
                &provider_globals(),
            )
            .unwrap();

        let serves = ctx.serves.borrow();
        let serve = serves.get("app").expect("serve registered");
        assert_eq!(
            serve.services,
            Some(vec![Service {
                name: "database".to_owned(),
                provider: provider.to_owned(),
            }])
        );
    }
}
