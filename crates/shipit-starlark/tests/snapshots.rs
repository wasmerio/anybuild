//! Evaluate every committed Python compatibility case and compare its
//! Rust plan byte-for-byte against `tests/plan_snapshots/*.json`.

use std::path::PathBuf;

use serde::Deserialize;
use shipit_plan::layout::LocalLayout;
use shipit_plan::snapshot;
use shipit_starlark::eval::{evaluate_shipit, EvaluateOptions};
use shipit_starlark::loader::StdlibSource;

const EXPECTED_SNAPSHOT_CASES: usize = 98;
const EXPECTED_LEGACY_CASES: usize = 80;
const ALLOW_MISSING_FIXTURES_ENV: &str = "SHIPIT_ALLOW_MISSING_FIXTURES";
const UPDATE_FIXTURES_ENV: &str = "SHIPIT_UPDATE_FIXTURES";

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
    let manifest_path = std::env::var("SHIPIT_FIXTURES")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/manifest.json")
        });
    if !manifest_path.is_file() && std::env::var(ALLOW_MISSING_FIXTURES_ENV).as_deref() == Ok("1") {
        eprintln!(
            "skipping: fixture manifest {} is missing and \
             {ALLOW_MISSING_FIXTURES_ENV}=1",
            manifest_path.display()
        );
        return;
    }
    assert!(
        manifest_path.is_file(),
        "fixture manifest {} is missing; restore the committed fixtures or set \
         {ALLOW_MISSING_FIXTURES_ENV}=1 to skip this gate locally",
        manifest_path.display()
    );
    let mut manifest: Manifest =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).expect("manifest readable"))
            .expect("manifest parses");
    assert_eq!(
        manifest.cases.len(),
        EXPECTED_SNAPSHOT_CASES,
        "fixture coverage changed; review the manifest and update the pinned count intentionally"
    );
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

    // Regeneration mode: rewrite the plan-snapshot goldens with the
    // evaluated output instead of comparing (see scripts/update_fixtures.sh).
    let update = std::env::var(UPDATE_FIXTURES_ENV).as_deref() == Ok("1");
    let mut updated = 0usize;
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
        let rendered = snapshot::render(&serve, &build_path, &shipit_dir, &case.workspace);
        let golden_path = manifest.snapshots.join(format!("{}.json", case.name));
        if update {
            let previous = std::fs::read_to_string(&golden_path).ok();
            if previous.as_deref() != Some(rendered.as_str()) {
                updated += 1;
            }
            std::fs::write(&golden_path, &rendered).unwrap();
            passed += 1;
            continue;
        }
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

    if update {
        eprintln!(
            "snapshots: rewrote {passed}/{} ({updated} changed); legacy: {legacy_ok}/{legacy_total} evaluate",
            manifest.cases.len()
        );
    } else {
        eprintln!(
            "snapshots: {passed}/{} matched; legacy: {legacy_ok}/{legacy_total} evaluate",
            manifest.cases.len()
        );
    }
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
            if failures.len() > 5 {
                "\n… (more)"
            } else {
                ""
            }
        );
    }
    assert_eq!(passed, EXPECTED_SNAPSHOT_CASES);
    assert_eq!(legacy_total, EXPECTED_LEGACY_CASES);
    assert_eq!(legacy_ok, EXPECTED_LEGACY_CASES);
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
