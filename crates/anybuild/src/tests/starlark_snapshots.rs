//! Evaluate every committed compatibility case and compare its
//! Rust plan byte-for-byte against `tests/plan_snapshots/*.json`.

use std::path::PathBuf;

use crate::plan::layout::LocalLayout;
use crate::plan::snapshot;
use crate::sdk::CommandOverrides;
use crate::starlark::config::ConfigResolutionOptions;
use crate::starlark::eval::{evaluate_anybuild, EvaluateOptions};
use crate::starlark::loader::StdlibSource;
use serde::Deserialize;

const EXPECTED_SNAPSHOT_CASES: usize = 98;
const ALLOW_MISSING_FIXTURES_ENV: &str = "ANYBUILD_ALLOW_MISSING_FIXTURES";
const UPDATE_FIXTURES_ENV: &str = "ANYBUILD_UPDATE_FIXTURES";

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
    subdir: Option<String>,
    #[serde(alias = "shipit")]
    anybuild: String,
    config: serde_json::Value,
}

#[test]
fn plan_snapshots_match() {
    let manifest_path = std::env::var("ANYBUILD_FIXTURES")
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
        let anybuild_file = work_dir.join("Anybuild");
        std::fs::write(&anybuild_file, &case.anybuild).unwrap();
        let anybuild_dir = tmp.path().join(".anybuild");
        let layout = LocalLayout::new(&anybuild_dir);
        let build_path = layout.build_path();

        let paths =
            crate::internal::paths::resolve_project_paths(&case.workspace, case.subdir.as_deref())
                .unwrap();
        let result = evaluate_anybuild(EvaluateOptions {
            anybuild_file,
            project_root: case.workspace.clone(),
            source_dir: paths.app_path.clone(),
            config_resolution: ConfigResolutionOptions {
                paths,
                overrides: CommandOverrides {
                    config: Some(case.config.clone()),
                    ..CommandOverrides::default()
                },
                runner: None,
                operation: crate::operation::OperationContext::for_test(),
            },
            layout: Box::new(layout),
            stdlib: StdlibSource::Dir(manifest.starlib.clone()),
        });
        let serve = match result {
            Ok(evaluated) => evaluated.serve,
            Err(err) => {
                failures.push(format!("{}: evaluation failed: {err:#}", case.name));
                continue;
            }
        };
        let rendered = snapshot::render(&serve, &build_path, &anybuild_dir, &case.workspace);
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

    if update {
        eprintln!(
            "snapshots: rewrote {passed}/{} ({updated} changed)",
            manifest.cases.len()
        );
    } else {
        eprintln!("snapshots: {passed}/{} matched", manifest.cases.len());
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
