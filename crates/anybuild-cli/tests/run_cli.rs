//! CLI-level checks driven through the built binary
//! (the counterpart of test_cli_after_deploy.py's typer-runner tests
//! that assert on process output).

#[test]
fn version_is_plain_text() {
    for option in ["--version", "-v"] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
            .arg(option)
            .output()
            .unwrap();

        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            format!("{}\n", env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    }
}

#[test]
fn legacy_binary_alias_is_available() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_shipit"))
        .arg("--version")
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("{}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn plan_command_delegates_to_the_sdk_contract() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("index.html"), "<h1>test</h1>\n").unwrap();
    let sdk = anybuild::Anybuild::new(tmp.path()).with_provider("staticfile");
    sdk.generate(Default::default()).unwrap();
    let expected = sdk.plan(Default::default()).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
        .arg("plan")
        .arg(tmp.path())
        .args(["--provider", "staticfile"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(actual["provider"], expected.provider);
    assert_eq!(actual["config"], expected.config);
}

#[test]
fn plan_reports_the_detected_provider_and_its_details() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("package.json"),
        r#"{"scripts":{"build":"next build","start":"next start"},"dependencies":{"next":"15.0.0"}}"#,
    )
    .unwrap();
    std::fs::write(tmp.path().join("package-lock.json"), "{}").unwrap();
    anybuild::Anybuild::new(tmp.path())
        .with_provider("node")
        .generate(Default::default())
        .unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
        .arg("plan")
        .arg(tmp.path())
        .args(["--provider", "node"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains("Detected Node.js provider"), "{stderr}");
    assert!(stderr.contains("  Framework: Next.js"), "{stderr}");
    assert!(stderr.contains("  Package manager: npm"), "{stderr}");
    assert!(stderr.contains("  Node version: 24"), "{stderr}");
}

#[test]
fn legacy_project_file_and_state_directory_are_renamed() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("index.html"), "<h1>test</h1>\n").unwrap();
    std::fs::write(
        tmp.path().join("Shipit"),
        r#"load("//shipit/tools:staticfile.shipit", "staticfile_build", "staticfile_serve")

build = staticfile_build(config)
staticfile_serve(config, build, name = "legacy")
"#,
    )
    .unwrap();
    std::fs::create_dir(tmp.path().join(".shipit")).unwrap();
    std::fs::write(tmp.path().join(".shipit/marker"), "legacy").unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
        .arg("plan")
        .arg(tmp.path())
        .args(["--provider", "staticfile"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(
        stderr.contains("Renamed legacy Shipit file to Anybuild."),
        "{stderr}"
    );
    assert!(
        stderr.contains("Renamed legacy .shipit directory to .anybuild."),
        "{stderr}"
    );
    assert!(tmp.path().join("Anybuild").is_file());
    assert!(!tmp.path().join("Shipit").exists());
    assert!(tmp.path().join(".anybuild/marker").is_file());
    assert!(!tmp.path().join(".shipit").exists());
}

#[test]
fn legacy_subdir_project_file_is_renamed() {
    let tmp = tempfile::tempdir().unwrap();
    let app = tmp.path().join("apps/site");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(app.join("index.html"), "<h1>test</h1>\n").unwrap();
    std::fs::write(
        tmp.path().join("Shipit.apps-site"),
        r#"load("//shipit/tools:staticfile.shipit", "staticfile_build", "staticfile_serve")

app_subdir = "apps/site"

build = staticfile_build(config)
staticfile_serve(config, build, name = "legacy-subdir")
"#,
    )
    .unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
        .arg("plan")
        .arg(tmp.path())
        .args(["--subdir", "apps/site", "--provider", "staticfile"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(
        stderr.contains("Renamed legacy Shipit.apps-site file to Anybuild.apps-site."),
        "{stderr}"
    );
    assert!(tmp.path().join("Anybuild.apps-site").is_file());
    assert!(!tmp.path().join("Shipit.apps-site").exists());
}

/// Port of tests/test_cli_after_deploy.py::
/// test_run_without_commands_prints_to_stderr.
#[test]
fn run_without_commands_prints_to_stderr() {
    let tmp = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
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

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
        .arg("generate")
        .arg(tmp.path())
        .args(["--provider", "staticfile", "--out"])
        .arg(tmp.path().join("Anybuild.generated"))
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
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
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
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
        .arg("generate")
        .arg(tmp.path())
        .args(["--provider", "node-static"])
        .env("ANYBUILD_FRAMEWORK", "express")
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
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
            .arg("generate")
            .arg(tmp.path())
            .args(["--provider", "node", "--out"])
            .arg(tmp.path().join(format!("Anybuild.{field}")))
            .env(format!("ANYBUILD_{field}"), value)
            .output()
            .unwrap();

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{field}: {stderr}");
        assert!(
            stderr.contains(&format!("Invalid value for ANYBUILD_{field}")),
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
        tmp.path().join("Anybuild"),
        r#"load("//anybuild/tools:staticfile.bzl", "staticfile_build", "staticfile_serve")

build = staticfile_build(config)
staticfile_serve(config, build, name = "empty-env")
"#,
    )
    .unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
        .arg("plan")
        .arg(tmp.path())
        .args(["--provider", "staticfile"])
        .env("ANYBUILD_SWS_VERSION", "")
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

#[test]
fn anybuild_env_takes_precedence_and_shipit_env_remains_supported() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("index.html"), "<h1>test</h1>\n").unwrap();
    std::fs::write(
        tmp.path().join("Anybuild"),
        r#"load("//anybuild/tools:staticfile.bzl", "staticfile_build", "staticfile_serve")

build = staticfile_build(config)
staticfile_serve(config, build, name = "env-precedence")
"#,
    )
    .unwrap();

    let legacy = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
        .arg("plan")
        .arg(tmp.path())
        .args(["--provider", "staticfile"])
        .env("SHIPIT_SWS_VERSION", "legacy")
        .output()
        .unwrap();
    assert!(legacy.status.success());
    assert!(String::from_utf8_lossy(&legacy.stdout).contains(r#""sws_version": "legacy""#));

    let preferred = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
        .arg("plan")
        .arg(tmp.path())
        .args(["--provider", "staticfile"])
        .env("ANYBUILD_SWS_VERSION", "")
        .env("SHIPIT_SWS_VERSION", "legacy")
        .output()
        .unwrap();
    assert!(preferred.status.success());
    let stdout = String::from_utf8_lossy(&preferred.stdout);
    assert!(stdout.contains(r#""sws_version": """#), "{stdout}");
    assert!(!stdout.contains(r#""sws_version": "legacy""#), "{stdout}");
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

    // ANYBUILD_NAME="" must fall back to the directory name (generator.py:57),
    // and ANYBUILD_WP_VERSION="" must not force wordpress detection
    // (wordpress.py only detects on a truthy version).
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
        .arg("plan")
        .arg(&project)
        .env("ANYBUILD_NAME", "")
        .env("ANYBUILD_WP_VERSION", "")
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
    let copied_binary = tmp.path().join("anybuild-copy");
    std::fs::copy(env!("CARGO_BIN_EXE_anybuild"), &copied_binary).unwrap();

    let project = tmp.path().join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("index.html"), "<h1>relocatable</h1>\n").unwrap();
    std::fs::write(
        project.join("Anybuild"),
        r#"load("//anybuild/tools:staticfile.bzl", "staticfile_build", "staticfile_serve")

build = staticfile_build(config)
staticfile_serve(config, build, name = "relocatable")
"#,
    )
    .unwrap();

    let output = std::process::Command::new(&copied_binary)
        .arg("plan")
        .arg(&project)
        .args(["--provider", "staticfile"])
        .env_remove("ANYBUILD_STARLIB")
        .env_remove("ANYBUILD_ASSETS")
        .current_dir(tmp.path())
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(String::from_utf8_lossy(&output.stdout).contains(r#""provider": "staticfile""#));
    assert!(!project.join(".anybuild/runtime").exists());
}
