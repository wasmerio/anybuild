//! CLI-level checks driven through the built binary
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

#[test]
fn invalid_redirects_are_reported_without_panicking() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("index.html"), "<h1>test</h1>\n").unwrap();
    std::fs::write(
        tmp.path().join("_redirects"),
        "/docs/* /guides/:splat/ 301 Country=us\n",
    )
    .unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_shipit"))
        .arg("generate")
        .arg(tmp.path())
        .args(["--provider", "staticfile", "--out"])
        .arg(tmp.path().join("Shipit.generated"))
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(stderr.contains("Error:"), "{stderr}");
    assert!(
        stderr.contains("conditions and forced redirects are not supported"),
        "{stderr}"
    );
    assert!(!stderr.contains("panicked at"), "{stderr}");
}
