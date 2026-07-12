//! CLI-level `shipit run` checks driven through the built binary
//! (the counterpart of test_cli_after_deploy.py's typer-runner tests
//! that assert on process output).

/// Port of tests/test_cli_after_deploy.py::
/// test_run_without_commands_prints_to_stderr.
#[test]
fn run_without_commands_prints_to_stderr() {
    let tmp = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_shipit"))
        .arg("run")
        .arg(tmp.path())
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert!(stderr.contains("No commands specified"), "{stderr}");
}
