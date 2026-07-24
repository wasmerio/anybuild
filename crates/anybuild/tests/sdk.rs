use std::sync::{Arc, Mutex};

use anybuild::{
    Anybuild, AutoOptions, BuildOptions, DeployOptions, DeployOutcome, DeployTarget, Event,
    GenerateOptions, GenerationPolicy, PlanOptions, ProcessIo, RunOptions, WasmerOptions,
};

fn static_project() -> tempfile::TempDir {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("index.html"), "<h1>SDK</h1>\n").unwrap();
    project
}

#[test]
fn generate_and_plan_return_structured_data() {
    let project = static_project();
    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&events);
    let generated = Anybuild::new(project.path())
        .with_provider("staticfile")
        .with_event_handler(move |event: &Event| captured.lock().unwrap().push(event.clone()))
        .generate(GenerateOptions::default())
        .unwrap();

    assert_eq!(generated.provider, "staticfile");
    assert_eq!(
        generated.path,
        project.path().join("Anybuild").canonicalize().unwrap()
    );
    assert!(generated.content.contains("staticfile_build"));
    let events = events.lock().unwrap();
    assert!(matches!(
        events.as_slice(),
        [
            Event::ProviderDetected { .. },
            Event::FileWritten {
                kind: "anybuild",
                ..
            }
        ]
    ));

    let plan = Anybuild::new(project.path())
        .with_provider("staticfile")
        .plan(PlanOptions::default())
        .unwrap();
    assert_eq!(plan.provider, "staticfile");
    assert_eq!(plan.serve.provider, "staticfile");
    assert!(plan.serve.commands["start"].contains("static-web-server"));
}

#[test]
fn env_files_layer_from_workspace_to_subdir_and_named_environment() {
    let project = static_project();
    let app = project.path().join("apps/site");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(app.join("index.html"), "<h1>subdir</h1>").unwrap();
    std::fs::write(project.path().join(".env"), "VALUE=root\n").unwrap();
    std::fs::write(app.join(".env"), "VALUE=app\n").unwrap();
    std::fs::write(app.join(".env.prod"), "VALUE=prod\n").unwrap();
    std::fs::write(
        project.path().join("Anybuild.apps-site"),
        r#"app_subdir = "apps/site"
serve(
    name = "env",
    provider = "staticfile",
    build = [],
    deps = [],
    commands = {"start": "true"},
    env = {},
)
"#,
    )
    .unwrap();

    let outcome = Anybuild::new(project.path())
        .with_subdir("apps/site")
        .with_provider("staticfile")
        .build(BuildOptions {
            env_name: Some("prod".to_owned()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(outcome.plan.serve.env.unwrap_or_default()["VALUE"], "prod");
}

#[test]
fn environment_is_snapshotted_and_overrides_are_isolated() {
    let project = static_project();
    let sdk = Anybuild::new(project.path())
        .inherit_process_env(false)
        .with_env("ANYBUILD_SWS_VERSION", "sdk-version")
        .with_env("SHIPIT_SWS_VERSION", "legacy-version")
        .with_provider("staticfile");
    sdk.generate(GenerateOptions::default()).unwrap();
    let plan = sdk.plan(PlanOptions::default()).unwrap();
    assert_eq!(plan.config["sws_version"], "sdk-version");
    assert_ne!(
        std::env::var("ANYBUILD_SWS_VERSION").ok().as_deref(),
        Some("sdk-version")
    );
}

#[test]
fn compatibility_renames_are_reported_as_events() {
    let project = static_project();
    std::fs::write(
        project.path().join("Shipit"),
        r#"serve(
    name = "legacy",
    provider = "staticfile",
    build = [],
    deps = [],
    commands = {"start": "true"},
)
"#,
    )
    .unwrap();
    std::fs::create_dir(project.path().join(".shipit")).unwrap();

    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&events);
    Anybuild::new(project.path())
        .with_provider("staticfile")
        .with_event_handler(move |event: &Event| captured.lock().unwrap().push(event.clone()))
        .plan(PlanOptions::default())
        .unwrap();

    let events = events.lock().unwrap();
    assert!(events.iter().any(|event| matches!(
        event,
        Event::LegacyRenamed { from, to }
            if from.ends_with("Shipit") && to.ends_with("Anybuild")
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::LegacyRenamed { from, to }
            if from.ends_with(".shipit") && to.ends_with(".anybuild")
    )));
}

#[test]
fn build_run_and_auto_use_the_library_pipeline() {
    let project = static_project();
    std::fs::write(
        project.path().join("Anybuild"),
        r#"serve(
    name = "sdk",
    provider = "staticfile",
    build = [],
    deps = [],
    commands = {"start": "true", "probe": "true"},
)
"#,
    )
    .unwrap();
    let sdk = Anybuild::new(project.path()).with_provider("staticfile");
    let build = sdk.build(BuildOptions::default()).unwrap();
    assert_eq!(build.plan.provider, "staticfile");
    assert!(build.state_dir.ends_with(".anybuild"));

    let run = sdk
        .run(
            RunOptions::default()
                .command("probe")
                .volume("cache", "/cache"),
        )
        .unwrap();
    assert_eq!(run.executed, ["probe"]);

    let auto = Anybuild::new(project.path())
        .with_provider("staticfile")
        .auto(AutoOptions {
            generation: GenerationPolicy::Always,
            ..Default::default()
        })
        .unwrap();
    assert!(auto.generated.is_some());
    assert_eq!(auto.build.plan.provider, "staticfile");
}

#[cfg(unix)]
#[test]
fn deploy_config_can_use_piped_process_events() {
    use std::os::unix::fs::PermissionsExt;

    let project = static_project();
    let state = project.path().join(".anybuild/wasmer");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(state.join("app.yaml"), "kind: wasmer.io/App.v0\n").unwrap();
    let fake_wasmer = project.path().join("fake-wasmer");
    std::fs::write(
        &fake_wasmer,
        "#!/bin/sh\nout=''\nwhile [ \"$#\" -gt 0 ]; do\n  if [ \"$1\" = '--out' ]; then shift; out=\"$1\"; fi\n  shift\ndone\nprintf webc > \"$out\"\necho \"packaged:$SDK_SECRET\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&fake_wasmer, std::fs::Permissions::from_mode(0o755)).unwrap();
    let config = project.path().join("deploy.json");
    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&events);
    let outcome = Anybuild::new(project.path())
        .with_env("SDK_SECRET", "do-not-leak")
        .with_event_handler(move |event: &Event| captured.lock().unwrap().push(event.clone()))
        .deploy(DeployOptions {
            wasmer: WasmerOptions {
                binary: Some(fake_wasmer.display().to_string()),
                ..Default::default()
            },
            target: DeployTarget::WriteConfig {
                path: config.clone(),
            },
            process_io: ProcessIo::Events,
        })
        .unwrap();

    assert!(matches!(outcome, DeployOutcome::ConfigWritten { .. }));
    assert!(config.is_file());
    let events = events.lock().unwrap();
    assert!(events.iter().any(
        |event| matches!(event, Event::ProcessOutput { text, .. } if text.contains("packaged:[REDACTED]"))
    ));
    assert!(!format!("{events:?}").contains("do-not-leak"));
}
