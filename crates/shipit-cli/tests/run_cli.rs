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

#[test]
fn missing_go_entrypoint_is_reported_without_panicking() {
    let tmp = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_shipit"))
        .arg("generate")
        .arg(tmp.path())
        .args(["--provider", "go"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(stderr.contains("No Go build file was found"), "{stderr}");
    assert!(!stderr.contains("panicked at"), "{stderr}");
}

#[test]
fn runtime_node_framework_is_rejected_by_node_static_without_panicking() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("package.json"),
        r#"{"scripts":{"build":"vite build"}}"#,
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_shipit"))
        .arg("generate")
        .arg(tmp.path())
        .args(["--provider", "node-static"])
        .env("SHIPIT_FRAMEWORK", "express")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(
        stderr.contains("does not have a static output directory"),
        "{stderr}"
    );
    assert!(!stderr.contains("panicked at"), "{stderr}");
}

#[test]
fn malformed_typed_env_overrides_are_reported_as_errors() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("package.json"), r#"{"main":"index.js"}"#).unwrap();

    for (field, value) in [
        ("PORT", "many"),
        ("USE_EDGEJS", "enabled"),
        ("FRAMEWORK", "not-a-framework"),
        ("EXTRA_DEPENDENCIES", "not-json"),
    ] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_shipit"))
            .arg("generate")
            .arg(tmp.path())
            .args(["--provider", "node", "--out"])
            .arg(tmp.path().join(format!("Shipit.{field}")))
            .env(format!("SHIPIT_{field}"), value)
            .output()
            .unwrap();

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{field}: {stderr}");
        assert!(
            stderr.contains(&format!("Invalid value for SHIPIT_{field}")),
            "{field}: {stderr}"
        );
        assert!(!stderr.contains("panicked at"), "{field}: {stderr}");
    }
}

#[test]
fn empty_string_env_override_is_preserved() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("index.html"), "<h1>test</h1>\n").unwrap();
    std::fs::write(
        tmp.path().join("Shipit"),
        r#"load("//shipit/tools:staticfile.shipit", "staticfile_build", "staticfile_serve")

build = staticfile_build(config)
staticfile_serve(config, build, name = "empty-env")
"#,
    )
    .unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_shipit"))
        .arg("plan")
        .arg(tmp.path())
        .args(["--provider", "staticfile"])
        .env("SHIPIT_SWS_VERSION", "")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains(r#""sws_version": """#),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

/// Python applies truthiness (not presence) at fallback sites, so an
/// explicitly empty env value still triggers the fallback even though the
/// field itself preserves "".
#[test]
fn empty_string_env_still_triggers_python_truthiness_fallbacks() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("truthy-proj");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("index.html"), "<h1>test</h1>\n").unwrap();

    // SHIPIT_NAME="" must fall back to the directory name (generator.py:57),
    // and SHIPIT_WP_VERSION="" must not force wordpress detection
    // (wordpress.py only detects on a truthy version).
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_shipit"))
        .arg("plan")
        .arg(&project)
        .env("SHIPIT_NAME", "")
        .env("SHIPIT_WP_VERSION", "")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(r#""name": "truthy-proj""#), "{stdout}");
    assert!(!stdout.contains(r#""wp_version""#), "{stdout}");
}

#[test]
fn copied_binary_uses_embedded_runtime_resources() {
    let tmp = tempfile::tempdir().unwrap();
    let copied_binary = tmp.path().join("shipit-copy");
    std::fs::copy(env!("CARGO_BIN_EXE_shipit"), &copied_binary).unwrap();

    let project = tmp.path().join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("index.html"), "<h1>relocatable</h1>\n").unwrap();
    std::fs::write(
        project.join("Shipit"),
        r#"load("//shipit/tools:staticfile.shipit", "staticfile_build", "staticfile_serve")

build = staticfile_build(config)
staticfile_serve(config, build, name = "relocatable")
"#,
    )
    .unwrap();

    let output = std::process::Command::new(&copied_binary)
        .arg("plan")
        .arg(&project)
        .args(["--provider", "staticfile"])
        .env_remove("SHIPIT_STARLIB")
        .env_remove("SHIPIT_ASSETS")
        .current_dir(tmp.path())
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(String::from_utf8_lossy(&output.stdout).contains(r#""provider": "staticfile""#));
    assert!(!project.join(".shipit/runtime").exists());
}
