//! Detection and config loading must produce the exact committed JSON.
//! the committed expected output for every compatibility case.
//!
//! Regeneration: `ANYBUILD_UPDATE_FIXTURES=1` rewrites each case's `config`
//! in `fixtures/manifest.json` with the computed value instead of
//! comparing (see `scripts/update_fixtures.sh`). Intentional config
//! changes are reviewed as fixture diffs, like any golden.

use std::path::PathBuf;

use crate::operation::OperationContext;
use crate::providers::{base::BaseConfig, select_provider, workspace};
use serde::Deserialize;

const EXPECTED_CONFIG_CASES: usize = 98;
const ALLOW_MISSING_FIXTURES_ENV: &str = "ANYBUILD_ALLOW_MISSING_FIXTURES";
const UPDATE_FIXTURES_ENV: &str = "ANYBUILD_UPDATE_FIXTURES";

#[derive(Deserialize)]
struct Manifest {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    workspace: PathBuf,
    #[serde(default)]
    subdir: Option<String>,
    config: serde_json::Value,
}

fn example_env(case_name: &str) -> Vec<(&'static str, &'static str)> {
    // Mirrors EXAMPLE_ENV in the dump script / test suites.
    if case_name.starts_with("php-wordpress-empty") {
        vec![
            ("ANYBUILD_WP_VERSION", "latest"),
            ("ANYBUILD_PHPIX", "true"),
        ]
    } else {
        vec![]
    }
}

#[test]
fn configs_match_python() {
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
        EXPECTED_CONFIG_CASES,
        "fixture coverage changed; review the manifest and update the pinned count intentionally"
    );
    // Committed fixtures carry repo-relative workspace paths.
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for case in &mut manifest.cases {
        if case.workspace.is_relative() {
            case.workspace = repo_root.join(&case.workspace);
        }
    }

    let update = std::env::var(UPDATE_FIXTURES_ENV).as_deref() == Ok("1");
    let mut failures: Vec<String> = Vec::new();
    let mut passed = 0usize;
    let mut computed: Vec<serde_json::Value> = Vec::new();

    for case in &manifest.cases {
        let env = example_env(&case.name);
        for (key, value) in &env {
            std::env::set_var(key, value);
        }
        let result = compute_config(case);
        for (key, _) in &env {
            std::env::remove_var(key);
        }
        match result {
            Ok(config) => {
                if update {
                    computed.push(config);
                } else if config == case.config {
                    passed += 1;
                } else {
                    failures.push(format!(
                        "{}: {}",
                        case.name,
                        diff_json(&case.config, &config)
                    ));
                }
            }
            Err(message) => failures.push(format!("{}: {message}", case.name)),
        }
    }

    if update {
        assert!(
            failures.is_empty(),
            "cannot update fixtures, {} case(s) failed to compute:\n{}",
            failures.len(),
            failures.join("\n---\n")
        );
        let mut raw: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
        let cases = raw["cases"].as_array_mut().expect("cases array");
        assert_eq!(cases.len(), computed.len());
        let total = cases.len();
        let mut changed = 0usize;
        for (case, config) in cases.iter_mut().zip(computed) {
            if case["config"] != config {
                changed += 1;
            }
            case["config"] = config;
        }
        std::fs::write(&manifest_path, serde_json::to_string_pretty(&raw).unwrap()).unwrap();
        eprintln!(
            "configs: rewrote {total} case(s) in {} ({changed} changed)",
            manifest_path.display()
        );
        return;
    }

    eprintln!("configs: {passed}/{} matched", manifest.cases.len());
    if !failures.is_empty() {
        let shown = failures
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n---\n");
        panic!(
            "{} of {} config cases failed:\n{shown}{}",
            failures.len(),
            manifest.cases.len(),
            if failures.len() > 8 {
                "\n… (more)"
            } else {
                ""
            }
        );
    }
    assert_eq!(passed, EXPECTED_CONFIG_CASES);
}

fn compute_config(case: &Case) -> Result<serde_json::Value, String> {
    let operation = OperationContext::for_test();
    let is_synthetic = case.name.starts_with("synthetic__");
    let is_cross = case.name.ends_with("__cross");
    let app_path = match &case.subdir {
        Some(subdir) => case.workspace.join(subdir),
        None => case.workspace.clone(),
    };

    let mut base = BaseConfig::default();
    base.commands.enrich_from_path(&app_path);

    let (_, mut config) = select_provider(&app_path, &base, None, &operation)
        .map_err(|e| format!("detection failed: {e:#}"))?;

    if is_cross && !config.set_cross_platform("wasix_wasm32") {
        return Err("provider has no cross_platform field".to_owned());
    }

    if is_synthetic {
        // _workspace_case sets app_subdir directly, no workspace config.
        if let Some(subdir) = &case.subdir {
            config.base_mut().app_subdir = Some(subdir.clone());
        }
    } else {
        workspace::apply_subdir_provider_config(&mut config, case.subdir.as_deref());
        config.apply_workspace_config(&case.workspace);
    }

    Ok(config.to_json())
}

fn diff_json(expected: &serde_json::Value, actual: &serde_json::Value) -> String {
    let (Some(expected_map), Some(actual_map)) = (expected.as_object(), actual.as_object()) else {
        return "config is not an object".to_owned();
    };
    let mut lines = Vec::new();
    for (key, expected_value) in expected_map {
        match actual_map.get(key) {
            None => lines.push(format!("  missing key {key:?} (expected {expected_value})")),
            Some(actual_value) if actual_value != expected_value => lines.push(format!(
                "  {key}: expected {expected_value}, got {actual_value}"
            )),
            _ => {}
        }
    }
    for key in actual_map.keys() {
        if !expected_map.contains_key(key) {
            lines.push(format!("  extra key {key:?}"));
        }
    }
    if lines.is_empty() {
        "values differ in nested structure".to_owned()
    } else {
        lines.truncate(12);
        lines.join("\n")
    }
}
