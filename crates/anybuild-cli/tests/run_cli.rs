//! CLI-level checks driven through the built binary
//! (the counterpart of test_cli_after_deploy.py's typer-runner tests
//! that assert on process output).

fn fake_wasmer(dir: &std::path::Path) -> std::path::PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let path = dir.join("fake-wasmer");
        std::fs::write(&path, "#!/bin/sh\necho 'wasmer 7.2.1'\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[cfg(windows)]
    {
        let path = dir.join("fake-wasmer.cmd");
        std::fs::write(&path, "@echo off\r\necho wasmer 7.2.1\r\n").unwrap();
        path
    }
}

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
fn generate_check_reports_drift_without_rewriting() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("index.html"), "<h1>test</h1>\n").unwrap();
    std::fs::write(tmp.path().join("Staticfile"), "root: public\n").unwrap();
    let generated = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
        .arg("generate")
        .arg(tmp.path())
        .args(["--provider", "staticfile"])
        .output()
        .unwrap();
    let generated_stderr = String::from_utf8_lossy(&generated.stderr);
    assert!(generated.status.success(), "{generated_stderr}");
    assert!(
        generated_stderr.contains("  Generated Anybuild at"),
        "{generated_stderr}"
    );
    assert!(
        !generated_stderr.contains("Generating Anybuild"),
        "{generated_stderr}"
    );
    assert!(
        !generated_stderr.contains("  Config:"),
        "{generated_stderr}"
    );
    assert_eq!(generated_stderr.matches("Generated Anybuild at").count(), 1);
    let before = std::fs::read_to_string(tmp.path().join("Anybuild")).unwrap();

    let current = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
        .arg("generate")
        .arg(tmp.path())
        .args(["--check", "--provider", "staticfile"])
        .output()
        .unwrap();
    assert!(
        current.status.success(),
        "{}",
        String::from_utf8_lossy(&current.stderr)
    );

    std::fs::write(tmp.path().join("Staticfile"), "root: dist\n").unwrap();
    let drifted = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
        .arg("generate")
        .arg(tmp.path())
        .args(["--check", "--provider", "staticfile"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&drifted.stderr);
    assert!(!drifted.status.success(), "{stderr}");
    assert!(stderr.contains("config.static_dir"), "{stderr}");
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("Anybuild")).unwrap(),
        before
    );
}

#[test]
fn new_anybuild_output_flows_from_detection_to_generation_to_packages() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("index.html"), "<h1>test</h1>\n").unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
        .arg("auto")
        .arg(tmp.path())
        .args(["--provider", "staticfile", "--skip-prepare"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");

    let detected = stderr.find("  Detected static site provider").unwrap();
    let generated = stderr.find("\n\n  Generated Anybuild at ").unwrap();
    let packages = stderr.find("\n\n  Packages\n").unwrap();
    assert!(detected < generated);
    assert!(generated < packages);
    assert_eq!(stderr.matches("Generated Anybuild at").count(), 1);
    assert!(!stderr.contains("Generating Anybuild"));
    assert!(!stderr.contains("  Config:"));
}

#[test]
fn plan_reports_the_declared_provider_and_its_details() {
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
    assert!(
        stderr.contains("  Using Node.js provider declared in Anybuild"),
        "{stderr}"
    );
    assert!(!stderr.contains("Detected Node.js provider"), "{stderr}");
    assert!(stderr.contains("    Framework: Next.js"), "{stderr}");
    assert!(stderr.contains("    Package manager: npm"), "{stderr}");
    assert!(stderr.contains("    Node version: 24"), "{stderr}");
}

#[test]
fn build_prints_the_complete_plan_summary() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("index.html"), "<h1>test</h1>\n").unwrap();
    std::fs::write(
        tmp.path().join("Anybuild"),
        r#"load("//anybuild/tools:staticfile.bzl", "staticfile_config")
config = staticfile_config()
serve(
    name = "summary",
    provider = config.provider,
    build = [
        use(dep("node", version = "24"), dep("npm")),
        env(CI = "true"),
        run("true", group = "install"),
        run("true", group = "build"),
        run("true"),
        copy("index.html"),
    ],
    deps = [dep("node", version = "24"), dep("static-web-server", version = "2.38.0")],
    commands = {"start": "true", "after_deploy": "true"},
    prepare = [run("true")],
)
"#,
    )
    .unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
        .arg("build")
        .arg(tmp.path())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(
        stderr.contains("╰─────────────────╯\n\n  Using static site provider declared in Anybuild"),
        "{stderr}"
    );
    assert!(stderr.contains("\n\n  Packages\n"), "{stderr}");
    assert!(stderr.contains("  Packages\n  ──────────"), "{stderr}");
    assert!(
        stderr.contains("npm                │  -       │  only build"),
        "{stderr}"
    );
    assert!(
        stderr.contains("static-web-server  │  2.38.0  │  only deploy"),
        "{stderr}"
    );
    assert!(stderr.contains("  Build Steps\n  ───────────"), "{stderr}");
    assert!(!stderr.contains("  ▸ environment"), "{stderr}");
    assert!(stderr.contains("  ▸ install\n    $ true"), "{stderr}");
    assert!(stderr.contains("  ▸ build\n    $ true"), "{stderr}");
    assert!(!stderr.contains("  ▸ copy"), "{stderr}");
    assert!(
        stderr.contains("  Prepare\n  ──────────\n    $ true"),
        "{stderr}"
    );
    assert!(
        stderr.contains("  Deploy scripts\n  ──────────────"),
        "{stderr}"
    );
    assert!(stderr.contains("  ▸ start\n    $ true"), "{stderr}");
    assert!(stderr.contains("  ▸ after_deploy\n    $ true"), "{stderr}");
    assert!(
        stderr.contains("  Starting Build...\n  ─────────────────"),
        "{stderr}"
    );
    assert!(
        regex::Regex::new(r"\n  │ Build complete in \d+\.\d{2}s │\n")
            .unwrap()
            .is_match(&stderr),
        "{stderr}"
    );
    assert!(!stderr.contains("Created prepare.sh script"), "{stderr}");
    assert!(stderr.contains("\n  Preparing\n  ──────────\n"), "{stderr}");
    assert!(
        stderr.contains("\n  Copy to index.html from index.html"),
        "{stderr}"
    );
    assert!(!stderr.contains(&"-".repeat(80)), "{stderr}");

    let detailed = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
        .arg("build")
        .arg(tmp.path())
        .arg("--show-detailed-steps")
        .output()
        .unwrap();
    let detailed_stderr = String::from_utf8_lossy(&detailed.stderr);
    assert!(detailed.status.success(), "{detailed_stderr}");
    assert!(
        detailed_stderr.contains("  ▸ copy\n    index.html → index.html"),
        "{detailed_stderr}"
    );
    assert!(
        detailed_stderr.contains("  ▸ environment\n    CI"),
        "{detailed_stderr}"
    );

    let run = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
        .arg("run")
        .arg(tmp.path())
        .arg("--start")
        .output()
        .unwrap();
    let run_stderr = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "{run_stderr}");
    assert!(
        run_stderr.contains("  Run start command\n  ─────────────────"),
        "{run_stderr}"
    );

    let wasmer_tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        wasmer_tmp.path().join("Anybuild"),
        r#"load("//anybuild/tools:staticfile.bzl", "staticfile_config")
config = staticfile_config()
serve(
    name = "wasmer-files",
    provider = config.provider,
    build = [],
    deps = [dep("static-web-server", version = "2.38.0")],
    commands = {"start": "static-web-server"},
)
"#,
    )
    .unwrap();
    let fake_wasmer = fake_wasmer(wasmer_tmp.path());

    let hidden_wasmer_files = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
        .arg("build")
        .arg(wasmer_tmp.path())
        .args(["--wasmer", "--skip-prepare", "--wasmer-bin"])
        .arg(&fake_wasmer)
        .output()
        .unwrap();
    let hidden_stderr = String::from_utf8_lossy(&hidden_wasmer_files.stderr);
    assert!(hidden_wasmer_files.status.success(), "{hidden_stderr}");
    assert!(
        !hidden_stderr.contains("Created wasmer.toml manifest"),
        "{hidden_stderr}"
    );
    assert!(
        !hidden_stderr.contains("Created app.yaml manifest"),
        "{hidden_stderr}"
    );
    assert!(!hidden_stderr.contains("[package]"), "{hidden_stderr}");
    assert!(!hidden_stderr.contains('✅'), "{hidden_stderr}");

    let shown_wasmer_files = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
        .arg("build")
        .arg(wasmer_tmp.path())
        .args([
            "--wasmer",
            "--skip-prepare",
            "--show-wasmer-files",
            "--wasmer-bin",
        ])
        .arg(&fake_wasmer)
        .output()
        .unwrap();
    let shown_stderr = String::from_utf8_lossy(&shown_wasmer_files.stderr);
    assert!(shown_wasmer_files.status.success(), "{shown_stderr}");
    assert!(
        shown_stderr.contains("  │ Created wasmer.toml manifest │"),
        "{shown_stderr}"
    );
    assert!(
        shown_stderr.contains("  │ Created app.yaml manifest │"),
        "{shown_stderr}"
    );
    assert!(shown_stderr.contains("[package]"), "{shown_stderr}");
}

#[test]
fn legacy_project_file_and_state_directory_are_renamed() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("index.html"), "<h1>test</h1>\n").unwrap();
    std::fs::write(
        tmp.path().join("Shipit"),
        r#"load("//shipit/tools:staticfile.shipit", "staticfile_build", "staticfile_config", "staticfile_serve")

config = staticfile_config()
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
        r#"load("//shipit/tools:staticfile.shipit", "staticfile_build", "staticfile_config", "staticfile_serve")

app_subdir = "apps/site"

config = staticfile_config()
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
fn non_static_node_framework_is_rejected_without_panicking() {
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
        .output()
        .unwrap();
    assert!(output.status.success());
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
        .arg("plan")
        .arg(tmp.path())
        .args(["--provider", "node-static"])
        .env("ANYBUILD_NODE_FRAMEWORK", "nestjs")
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
        ("EDGEJS_ENABLE", "enabled"),
        ("NODE_FRAMEWORK", "not-a-framework"),
        ("NODE_EXTRA_DEPENDENCIES", "not-json"),
    ] {
        let generated = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
            .arg("generate")
            .arg(tmp.path())
            .args(["--provider", "node", "--out"])
            .arg(tmp.path().join(format!("Anybuild.{field}")))
            .output()
            .unwrap();
        assert!(generated.status.success());
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_anybuild"))
            .arg("plan")
            .arg(tmp.path())
            .args(["--provider", "node", "--anybuild-path"])
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
        r#"load("//anybuild/tools:staticfile.bzl", "staticfile_build", "staticfile_config", "staticfile_serve")

config = staticfile_config()
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
        r#"load("//anybuild/tools:staticfile.bzl", "staticfile_build", "staticfile_config", "staticfile_serve")

config = staticfile_config()
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
    anybuild::Anybuild::new(&project)
        .with_provider("staticfile")
        .generate(Default::default())
        .unwrap();

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
        r#"load("//anybuild/tools:staticfile.bzl", "staticfile_build", "staticfile_config", "staticfile_serve")

config = staticfile_config()
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
