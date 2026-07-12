//! The phase-2 gate: detection + config loading must produce the exact
//! JSON config the Python implementation dumped for every snapshot case.
//!
//! Run `uv run python scripts/dump_rust_fixtures.py` first.

use std::path::PathBuf;

use serde::Deserialize;
use shipit_providers::{base::BaseConfig, load_provider, load_provider_config, workspace};

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
            ("SHIPIT_WP_VERSION", "latest"),
            ("SHIPIT_PHPIX", "true"),
        ]
    } else {
        vec![]
    }
}

#[test]
fn configs_match_python() {
    let manifest_path = match std::env::var("SHIPIT_FIXTURES") {
        Ok(path) => PathBuf::from(path),
        Err(_) => {
            let default = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/manifest.json");
            if !default.is_file() {
                eprintln!("skipping: no fixtures (run scripts/dump_rust_fixtures.py)");
                return;
            }
            default
        }
    };
    let mut manifest: Manifest = serde_json::from_str(
        &std::fs::read_to_string(&manifest_path).expect("manifest readable"),
    )
    .expect("manifest parses");
    // Committed fixtures carry repo-relative workspace paths.
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for case in &mut manifest.cases {
        if case.workspace.is_relative() {
            case.workspace = repo_root.join(&case.workspace);
        }
    }

    let mut failures: Vec<String> = Vec::new();
    let mut passed = 0usize;

    for case in &manifest.cases {
        let env = example_env(&case.name);
        for (key, value) in &env {
            std::env::set_var(key, value);
        }
        let result = run_case(case);
        for (key, _) in &env {
            std::env::remove_var(key);
        }
        match result {
            Ok(()) => passed += 1,
            Err(message) => failures.push(format!("{}: {message}", case.name)),
        }
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
            if failures.len() > 8 { "\n… (more)" } else { "" }
        );
    }
    assert!(passed > 0, "no cases ran");
}

fn run_case(case: &Case) -> Result<(), String> {
    let is_synthetic = case.name.starts_with("synthetic__");
    let is_cross = case.name.ends_with("__cross");
    let app_path = match &case.subdir {
        Some(subdir) => case.workspace.join(subdir),
        None => case.workspace.clone(),
    };

    let mut base = BaseConfig::default();
    base.commands.enrich_from_path(&app_path);

    let provider = load_provider(&app_path, &base, None)
        .map_err(|e| format!("detection failed: {e:#}"))?;
    let mut config = load_provider_config(provider, &app_path, base)
        .map_err(|e| format!("load_config failed: {e:#}"))?;

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
        workspace::apply_subdir_workspace_config(&case.workspace, &mut config);
    }

    let rust_json = config.to_json();
    if rust_json != case.config {
        return Err(diff_json(&case.config, &rust_json));
    }
    Ok(())
}

fn diff_json(expected: &serde_json::Value, actual: &serde_json::Value) -> String {
    let (Some(expected_map), Some(actual_map)) = (expected.as_object(), actual.as_object())
    else {
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
