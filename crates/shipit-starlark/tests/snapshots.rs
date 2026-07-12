//! The phase-1 gate: evaluate every plan-snapshot case with configs
//! injected from the Python side and compare byte-for-byte against
//! `tests/plan_snapshots/*.json`.
//!
//! Run `uv run python scripts/dump_rust_fixtures.py` first (the
//! `scripts/rust_gate.sh` wrapper does both).

use std::path::PathBuf;

use serde::Deserialize;
use shipit_plan::layout::LocalLayout;
use shipit_plan::snapshot;
use shipit_starlark::eval::{evaluate_shipit, EvaluateOptions};
use shipit_starlark::loader::StdlibSource;

#[derive(Deserialize)]
struct Manifest {
    starlib: PathBuf,
    snapshots: PathBuf,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    workspace: PathBuf,
    shipit: String,
    config: serde_json::Value,
    #[serde(default)]
    legacy_shipit: Option<String>,
}

#[test]
fn plan_snapshots_match() {
    let manifest_path = match std::env::var("SHIPIT_FIXTURES") {
        Ok(path) => PathBuf::from(path),
        Err(_) => {
            let default = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/manifest.json");
            if !default.is_file() {
                eprintln!(
                    "skipping: no fixtures (run scripts/dump_rust_fixtures.py \
                     or set SHIPIT_FIXTURES)"
                );
                return;
            }
            default
        }
    };
    let mut manifest: Manifest = serde_json::from_str(
        &std::fs::read_to_string(&manifest_path).expect("manifest readable"),
    )
    .expect("manifest parses");
    // Committed fixtures carry repo-relative paths.
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    if manifest.starlib.is_relative() {
        manifest.starlib = repo_root.join(&manifest.starlib);
    }
    if manifest.snapshots.is_relative() {
        manifest.snapshots = repo_root.join(&manifest.snapshots);
    }
    for case in &mut manifest.cases {
        if case.workspace.is_relative() {
            case.workspace = repo_root.join(&case.workspace);
        }
    }

    let mut failures: Vec<String> = Vec::new();
    let mut passed = 0usize;

    for case in &manifest.cases {
        let tmp = tempfile::tempdir().expect("tempdir");
        let work_dir = tmp.path().join("eval");
        std::fs::create_dir_all(&work_dir).unwrap();
        let shipit_file = work_dir.join("Shipit");
        std::fs::write(&shipit_file, &case.shipit).unwrap();
        let shipit_dir = tmp.path().join(".shipit");
        let layout = LocalLayout::new(&shipit_dir);
        let build_path = layout.build_path();

        let result = evaluate_shipit(EvaluateOptions {
            shipit_file,
            project_root: Some(case.workspace.clone()),
            config: case.config.clone(),
            layout: Box::new(layout),
            stdlib: StdlibSource::Dir(manifest.starlib.clone()),
        });
        let serve = match result {
            Ok(serve) => serve,
            Err(err) => {
                failures.push(format!("{}: evaluation failed: {err:#}", case.name));
                continue;
            }
        };
        let rendered =
            snapshot::render(&serve, &build_path, &shipit_dir, &case.workspace);
        let golden_path = manifest.snapshots.join(format!("{}.json", case.name));
        let golden = match std::fs::read_to_string(&golden_path) {
            Ok(text) => text,
            Err(_) => {
                failures.push(format!(
                    "{}: missing golden {}",
                    case.name,
                    golden_path.display()
                ));
                continue;
            }
        };
        if rendered != golden {
            let diff = first_diff(&golden, &rendered);
            failures.push(format!("{}: plan differs\n{diff}", case.name));
        } else {
            passed += 1;
        }
    }

    // Legacy compat: every fully-inlined main-era Shipit file must still
    // evaluate (same bar the Python evaluator meets).
    let mut legacy_ok = 0usize;
    let mut legacy_total = 0usize;
    for case in &manifest.cases {
        let Some(legacy) = &case.legacy_shipit else {
            continue;
        };
        legacy_total += 1;
        let tmp = tempfile::tempdir().expect("tempdir");
        let work_dir = tmp.path().join("eval");
        std::fs::create_dir_all(&work_dir).unwrap();
        let shipit_file = work_dir.join("Shipit");
        std::fs::write(&shipit_file, legacy).unwrap();
        let layout = LocalLayout::new(tmp.path().join(".shipit"));
        match evaluate_shipit(EvaluateOptions {
            shipit_file,
            project_root: Some(case.workspace.clone()),
            config: case.config.clone(),
            layout: Box::new(layout),
            stdlib: StdlibSource::Dir(manifest.starlib.clone()),
        }) {
            Ok(_) => legacy_ok += 1,
            Err(err) => failures.push(format!(
                "{} (legacy): evaluation failed: {err:#}",
                case.name
            )),
        }
    }

    eprintln!(
        "snapshots: {passed}/{} matched; legacy: {legacy_ok}/{legacy_total} evaluate",
        manifest.cases.len()
    );
    if !failures.is_empty() {
        let shown = failures
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n---\n");
        panic!(
            "{} of {} snapshot cases failed:\n{shown}{}",
            failures.len(),
            manifest.cases.len(),
            if failures.len() > 5 { "\n… (more)" } else { "" }
        );
    }
    assert!(passed > 0, "no cases ran");
}

fn first_diff(expected: &str, actual: &str) -> String {
    for (i, (e, a)) in expected.lines().zip(actual.lines()).enumerate() {
        if e != a {
            return format!(
                "  first differing line {}:\n  expected: {e}\n  actual:   {a}",
                i + 1
            );
        }
    }
    format!(
        "  line counts differ: expected {}, actual {}",
        expected.lines().count(),
        actual.lines().count()
    )
}
