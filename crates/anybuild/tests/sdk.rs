use std::sync::{Arc, Mutex};

use anybuild::plan::Step;
use anybuild::{
    Anybuild, AutoOptions, AwsLambdaOptions, BuildOptions, DeployOptions, DeployOutcome,
    DeployTarget, DeploymentPlatform, Event, FlyOptions, GenerateOptions, GenerationCheckStatus,
    GenerationPolicy, PlanOptions, ProcessIo, RunOptions, RuntimeArtifact, WasmerOptions,
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
            Event::AnybuildGenerating { .. },
            Event::FileWritten {
                kind: "anybuild",
                ..
            }
        ]
    ));

    let plan_events = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&plan_events);
    let plan = Anybuild::new(project.path())
        .with_provider("staticfile")
        .with_event_handler(move |event: &Event| captured.lock().unwrap().push(event.clone()))
        .plan(PlanOptions::default())
        .unwrap();
    assert_eq!(plan.provider, "staticfile");
    assert_eq!(plan.serve.provider, "staticfile");
    assert!(plan.serve.commands["start"].contains("static-web-server"));
    let plan_events = plan_events.lock().unwrap();
    assert!(plan_events
        .iter()
        .any(|event| matches!(event, Event::ProviderDeclared { provider, .. } if provider == "staticfile")));
    assert!(!plan_events
        .iter()
        .any(|event| matches!(event, Event::ProviderDetected { .. })));
}

#[test]
fn provider_detection_includes_provider_specific_details() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("package.json"),
        r#"{"scripts":{"build":"next build","start":"next start"},"dependencies":{"next":"15.0.0"}}"#,
    )
    .unwrap();
    std::fs::write(project.path().join("package-lock.json"), "{}").unwrap();

    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&events);
    Anybuild::new(project.path())
        .with_event_handler(move |event: &Event| captured.lock().unwrap().push(event.clone()))
        .plan(PlanOptions::default())
        .unwrap();

    let events = events.lock().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::ProviderDetected { .. }))
            .count(),
        1
    );
    let Event::ProviderDetected { provider, details } = &events[0] else {
        panic!("expected provider detection event, got {:?}", events[0]);
    };
    assert_eq!(provider, "node");
    assert!(details
        .iter()
        .any(|detail| detail.label == "Framework" && detail.value == "Next.js"));
    assert!(details
        .iter()
        .any(|detail| detail.label == "Package manager" && detail.value == "npm"));
    assert!(details
        .iter()
        .any(|detail| detail.label == "Node version" && detail.value == "24"));
}

#[test]
fn node_static_build_preserves_hidden_output_directories() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("package.json"),
        r#"{"scripts":{"build":"vite build"},"devDependencies":{"vite":"8.2.0"}}"#,
    )
    .unwrap();
    std::fs::write(project.path().join("package-lock.json"), "{}\n").unwrap();

    let sdk = Anybuild::new(project.path()).with_provider("node-static");
    sdk.generate(GenerateOptions::default()).unwrap();
    let plan = sdk.plan(PlanOptions::default()).unwrap();

    assert!(
        plan.serve.build.iter().any(|step| {
            matches!(
                step,
                Step::Run(run) if run.command.starts_with("cp -R dist/. ")
            )
        }),
        "build steps: {:?}",
        plan.serve.build
    );
}

#[test]
fn node_static_subdirectory_build_respects_gitignore() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("web")).unwrap();
    std::fs::write(
        project.path().join("web/package.json"),
        r#"{"scripts":{"build":"vite build"},"devDependencies":{"vite":"8.2.0"}}"#,
    )
    .unwrap();
    std::fs::write(project.path().join("web/package-lock.json"), "{}\n").unwrap();
    std::fs::write(
        project.path().join("Anybuild"),
        r#"load("//anybuild/tools:node_static.bzl", "nodestatic_build", "nodestatic_config", "nodestatic_serve")

app_subdir = "web"
config = nodestatic_config(
    schema = 1,
    static_dir = "dist",
    node_package_manager = "npm",
    node_framework = "vite",
    node_server = "node",
    node_build_command = "npm run build",
    node_version = "24",
)
build = nodestatic_build(config)
nodestatic_serve(config, build, name = "web")
"#,
    )
    .unwrap();

    let plan = Anybuild::new(project.path())
        .plan(PlanOptions::default())
        .unwrap();

    assert_eq!(plan.provider, "node-static");
    assert!(plan.serve.build.iter().any(|step| {
        matches!(
            step,
            Step::Copy(copy)
                if copy.source == "."
                    && copy.target == "."
                    && copy.gitignore
        )
    }));

    let build_index = plan
        .serve
        .build
        .iter()
        .position(|step| {
            matches!(
                step,
                Step::Run(run) if run.command == "npm run build"
            )
        })
        .expect("Node build step");
    let publish_index = plan
        .serve
        .build
        .iter()
        .position(|step| {
            matches!(
                step,
                Step::Run(run) if run.command.starts_with("cp -R dist/. ")
            )
        })
        .expect("static artifact publish step");

    assert!(build_index < publish_index);
    assert!(
        plan.serve.commands["start"].starts_with("static-web-server "),
        "start command: {}",
        plan.serve.commands["start"]
    );
}

#[test]
fn nitro_projects_build_and_start_with_the_node_server_preset() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("package.json"),
        r#"{"scripts":{"build":"vite build"},"dependencies":{"@tanstack/react-start":"1.0.0","nitro":"3.0.0"}}"#,
    )
    .unwrap();
    std::fs::write(project.path().join("bun.lock"), "").unwrap();

    let sdk = Anybuild::new(project.path());
    sdk.generate(GenerateOptions::default()).unwrap();
    let plan = sdk.plan(PlanOptions::default()).unwrap();

    assert_eq!(plan.provider, "node");
    assert_eq!(plan.config["node_package_manager"], "bun");
    assert_eq!(
        plan.serve.commands["start"],
        "node .output/server/index.mjs"
    );
    assert!(plan.serve.build.iter().any(|step| {
        matches!(
            step,
            Step::Env(env)
                if env.variables.get("NITRO_PRESET").map(String::as_str)
                    == Some("node-server")
        )
    }));
    assert!(plan.serve.build.iter().any(|step| {
        matches!(
            step,
            Step::Run(run)
                if run.command
                    .contains("bunx optimize-deps@0.1.2 .output/server --replace")
        )
    }));
}

#[test]
fn pnpm_next_subdir_deploys_from_the_app_and_preserves_the_bundle() {
    let project = tempfile::tempdir().unwrap();
    let app = project.path().join("apps/site");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(
        app.join("package.json"),
        r#"{
  "name": "next-site",
  "private": true,
  "scripts": {"build": "next build", "start": "next start"},
  "dependencies": {"next": "16.1.7", "react": "19.2.3", "react-dom": "19.2.3"}
}"#,
    )
    .unwrap();
    std::fs::write(app.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n").unwrap();
    std::fs::write(
        app.join("pnpm-workspace.yaml"),
        "ignoredBuiltDependencies:\n  - sharp\n",
    )
    .unwrap();

    let sdk = Anybuild::new(project.path())
        .with_subdir("apps/site")
        .with_env("ANYBUILD_NODE_REMOVE_NATIVE_BINARIES", "true");
    sdk.generate(GenerateOptions::default()).unwrap();
    let plan = sdk.plan(PlanOptions::default()).unwrap();
    let steps = &plan.serve.build;
    let deploy_index = steps
        .iter()
        .position(|step| {
            matches!(
                step,
                Step::Run(run)
                    if run.command.contains("pnpm deploy --filter next-site")
            )
        })
        .expect("pnpm deploy step");

    assert!(matches!(
        &steps[deploy_index - 1],
        Step::Workdir(step) if step.path.ends_with("apps/site")
    ));
    assert!(matches!(&steps[deploy_index + 1], Step::Workdir(_)));
    assert!(!steps.iter().any(|step| {
        matches!(step, Step::Run(run) if run.command.contains("rm -rf .next-bundle"))
    }));
    let optimize_index = steps
        .iter()
        .position(|step| {
            matches!(
                step,
                Step::Run(run) if run.command.contains("optimize-node-modules.sh")
            )
        })
        .expect("node_modules optimizer step");
    assert!(optimize_index > deploy_index + 1);
    assert!(matches!(
        &steps[optimize_index],
        Step::Run(run) if run.command.ends_with(".next-bundle/node_modules")
    ));
    assert_eq!(plan.serve.commands["start"], "node .next-bundle/server.mjs");
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
        r#"load("//anybuild/tools:staticfile.bzl", "staticfile_config")
app_subdir = "apps/site"
config = staticfile_config()
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
fn persisted_config_is_stable_and_generation_check_reports_drift() {
    let project = static_project();
    std::fs::write(project.path().join("Staticfile"), "root: public\n").unwrap();
    let sdk = Anybuild::new(project.path())
        .inherit_process_env(false)
        .with_provider("staticfile")
        .with_env("ANYBUILD_STATIC_DIR", "runtime")
        .with_config(serde_json::json!({"static_dir": "cli"}));

    let generated = sdk.generate(GenerateOptions::default()).unwrap();
    assert!(generated.content.contains("static_dir = \"public\""));
    assert!(!generated.content.contains("runtime"));
    assert!(!generated.content.contains("static_dir = \"cli\""));
    assert_eq!(generated.config["static_dir"], "public");

    let plan = sdk.plan(PlanOptions::default()).unwrap();
    assert_eq!(plan.config["static_dir"], "cli");

    let current = sdk.check_generation(GenerateOptions::default()).unwrap();
    assert_eq!(current.status, GenerationCheckStatus::Current);

    std::fs::write(project.path().join("Staticfile"), "root: dist\n").unwrap();
    let drifted = sdk.check_generation(GenerateOptions::default()).unwrap();
    assert_eq!(drifted.status, GenerationCheckStatus::Drifted);
    assert!(drifted
        .differences
        .iter()
        .any(|difference| difference.path == "config.static_dir"));

    let plan = sdk.plan(PlanOptions::default()).unwrap();
    assert_eq!(plan.config["static_dir"], "cli");
}

#[test]
fn generation_check_reports_a_missing_definition_without_writing() {
    let project = static_project();
    let checked = Anybuild::new(project.path())
        .with_provider("staticfile")
        .check_generation(GenerateOptions::default())
        .unwrap();
    assert_eq!(checked.status, GenerationCheckStatus::Missing);
    assert!(!checked.path.exists());
}

#[test]
fn persisted_config_metadata_and_fields_are_validated() {
    let project = static_project();
    let generated = Anybuild::new(project.path())
        .with_provider("staticfile")
        .generate(GenerateOptions::default())
        .unwrap();

    let unsupported = generated.content.replace("schema = 1", "schema = 99");
    std::fs::write(project.path().join("Anybuild"), unsupported).unwrap();
    let error = Anybuild::new(project.path())
        .plan(PlanOptions::default())
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("Unsupported staticfile config schema 99"));

    let unknown = generated
        .content
        .replace("schema = 1,", "schema = 1,\n    unknown_field = True,");
    std::fs::write(project.path().join("Anybuild"), unknown).unwrap();
    let error = Anybuild::new(project.path())
        .plan(PlanOptions::default())
        .unwrap_err();
    assert!(error.to_string().contains("Unknown persisted config field"));

    std::fs::write(project.path().join("Anybuild"), generated.content).unwrap();
    let error = Anybuild::new(project.path())
        .with_provider("node")
        .plan(PlanOptions::default())
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("declares provider \"staticfile\""));
}

#[test]
fn compatibility_renames_are_reported_as_events() {
    let project = static_project();
    std::fs::write(
        project.path().join("Shipit"),
        r#"load("//anybuild/tools:staticfile.bzl", "staticfile_config")
config = staticfile_config()
serve(
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
        r#"load("//anybuild/tools:staticfile.bzl", "staticfile_config")
config = staticfile_config()
serve(
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
    assert!(matches!(
        build.artifact,
        RuntimeArtifact::Local { ref directory }
            if directory.ends_with(".anybuild/runner/local")
    ));
    assert!(build.state_dir.join("artifact.json").is_file());

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

#[test]
fn temporary_generation_is_scoped_to_the_operation() {
    let project = static_project();
    let outcome = Anybuild::new(project.path())
        .with_provider("staticfile")
        .auto(AutoOptions {
            generation: GenerationPolicy::Temporary,
            ..Default::default()
        })
        .unwrap();
    let generated = outcome.generated.expect("temporary definition is reported");
    assert!(!generated.path.exists());
    assert!(!project.path().join("Anybuild").exists());
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
            platform: DeploymentPlatform::Wasmer(WasmerOptions {
                binary: Some(fake_wasmer.display().to_string()),
                ..Default::default()
            }),
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

#[cfg(unix)]
#[test]
fn fly_deployment_uses_the_docker_artifact_and_redacts_its_token() {
    use std::os::unix::fs::PermissionsExt;

    let project = static_project();
    let state = project.path().join(".anybuild");
    let artifact_dir = state.join("runner/docker");
    std::fs::create_dir_all(&artifact_dir).unwrap();
    std::fs::write(artifact_dir.join("Dockerfile"), "FROM scratch\n").unwrap();
    std::fs::write(artifact_dir.join("Dockerfile.dockerignore"), "**\n").unwrap();
    std::fs::write(artifact_dir.join("port"), "8080\n").unwrap();
    std::fs::write(
        state.join("artifact.json"),
        serde_json::to_string_pretty(&RuntimeArtifact::Docker {
            directory: artifact_dir,
            image: "sdk-fly-app".to_owned(),
            context: project.path().to_path_buf(),
            platform: None,
        })
        .unwrap(),
    )
    .unwrap();
    let fake_fly = project.path().join("fake-flyctl");
    std::fs::write(&fake_fly, "#!/bin/sh\necho \"fly:$*:$FLY_API_TOKEN\"\n").unwrap();
    std::fs::set_permissions(&fake_fly, std::fs::Permissions::from_mode(0o755)).unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&events);

    let outcome = Anybuild::new(project.path())
        .with_event_handler(move |event: &Event| captured.lock().unwrap().push(event.clone()))
        .deploy(DeployOptions {
            platform: DeploymentPlatform::Fly(FlyOptions {
                binary: Some(fake_fly.display().to_string()),
                token: Some("fly-secret".to_owned()),
                app: Some("sdk-fly-app".to_owned()),
                config: None,
            }),
            target: DeployTarget::Publish {
                owner: None,
                name: None,
            },
            process_io: ProcessIo::Events,
        })
        .unwrap();

    assert!(matches!(
        outcome,
        DeployOutcome::Published { name: Some(name), .. } if name == "sdk-fly-app"
    ));
    let events = events.lock().unwrap();
    assert!(events.iter().any(
        |event| matches!(event, Event::ProcessOutput { text, .. } if text.contains("--local-only"))
    ));
    assert!(!format!("{events:?}").contains("fly-secret"));
}

#[cfg(unix)]
#[test]
fn aws_lambda_deployment_creates_then_updates_a_container_function() {
    use std::os::unix::fs::PermissionsExt;

    let project = static_project();
    let state = project.path().join(".anybuild");
    let artifact_dir = state.join("runner/docker");
    std::fs::create_dir_all(&artifact_dir).unwrap();
    std::fs::write(
        artifact_dir.join("Dockerfile"),
        "FROM scratch\nCOPY --from=public.ecr.aws/awsguru/aws-lambda-adapter:1.0.0 /lambda-adapter /opt/extensions/lambda-adapter\n",
    )
    .unwrap();
    std::fs::write(
        state.join("artifact.json"),
        serde_json::to_string_pretty(&RuntimeArtifact::Docker {
            directory: artifact_dir,
            image: "sdk-lambda".to_owned(),
            context: project.path().to_path_buf(),
            platform: Some("linux/amd64".to_owned()),
        })
        .unwrap(),
    )
    .unwrap();

    let command_log = project.path().join("commands.log");
    let repository_marker = project.path().join("repository-created");
    let function_marker = project.path().join("function-created");
    let fake_aws = project.path().join("fake-aws");
    std::fs::write(
        &fake_aws,
        r#"#!/bin/sh
printf 'aws %s\n' "$*" >> "$COMMAND_LOG"
case "$1 $2" in
  'ecr describe-repositories')
    if [ ! -f "$REPOSITORY_MARKER" ]; then
      echo 'RepositoryNotFoundException' >&2
      exit 254
    fi
    echo '123456789012.dkr.ecr.us-west-2.amazonaws.com/sdk-lambda'
    ;;
  'ecr create-repository')
    touch "$REPOSITORY_MARKER"
    echo '123456789012.dkr.ecr.us-west-2.amazonaws.com/sdk-lambda'
    ;;
  'ecr get-login-password') echo 'registry-password' ;;
  'lambda get-function')
    if [ ! -f "$FUNCTION_MARKER" ]; then
      echo 'ResourceNotFoundException' >&2
      exit 254
    fi
      echo '{"Configuration":{"PackageType":"Image"}}'
    ;;
  'lambda create-function')
    touch "$FUNCTION_MARKER"
    echo "created:$AWS_SECRET_ACCESS_KEY"
    ;;
  'lambda update-function-code') echo "updated:$AWS_SECRET_ACCESS_KEY" ;;
  *) echo "unexpected AWS command: $*" >&2; exit 1 ;;
esac
"#,
    )
    .unwrap();
    std::fs::set_permissions(&fake_aws, std::fs::Permissions::from_mode(0o755)).unwrap();

    let fake_docker = project.path().join("fake-docker");
    std::fs::write(
        &fake_docker,
        r#"#!/bin/sh
if [ "$1" = 'login' ]; then
  cat >/dev/null
fi
printf 'docker %s\n' "$*" >> "$COMMAND_LOG"
echo "docker:$*"
"#,
    )
    .unwrap();
    std::fs::set_permissions(&fake_docker, std::fs::Permissions::from_mode(0o755)).unwrap();

    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&events);
    let sdk = Anybuild::new(project.path())
        .with_env("COMMAND_LOG", command_log.display().to_string())
        .with_env("REPOSITORY_MARKER", repository_marker.display().to_string())
        .with_env("FUNCTION_MARKER", function_marker.display().to_string())
        .with_env("AWS_SECRET_ACCESS_KEY", "aws-secret")
        .with_event_handler(move |event: &Event| captured.lock().unwrap().push(event.clone()));
    let options = |role: Option<&str>| DeployOptions {
        platform: DeploymentPlatform::AwsLambda(AwsLambdaOptions {
            binary: Some(fake_aws.display().to_string()),
            docker_binary: Some(fake_docker.display().to_string()),
            region: Some("us-west-2".to_owned()),
            function: Some("sdk-lambda".to_owned()),
            role: role.map(str::to_owned),
            ..Default::default()
        }),
        target: DeployTarget::Publish {
            owner: None,
            name: None,
        },
        process_io: ProcessIo::Events,
    };

    sdk.deploy(options(Some(
        "arn:aws:iam::123456789012:role/lambda-execution",
    )))
    .unwrap();
    sdk.deploy(options(None)).unwrap();

    let log = std::fs::read_to_string(command_log).unwrap();
    assert!(log.contains("ecr create-repository"));
    assert!(log.contains("docker login --username AWS --password-stdin"));
    assert!(log.contains("docker tag sdk-lambda"));
    assert!(log
        .contains("docker push 123456789012.dkr.ecr.us-west-2.amazonaws.com/sdk-lambda:anybuild"));
    assert!(log.contains("lambda create-function"));
    assert!(log.contains("--package-type Image"));
    assert!(log.contains("--architectures x86_64"));
    assert!(log.contains("lambda update-function-code"));
    let events = events.lock().unwrap();
    assert!(!format!("{events:?}").contains("aws-secret"));
}

#[cfg(unix)]
#[test]
fn aws_lambda_deployment_creates_then_updates_a_managed_runtime_function() {
    use std::os::unix::fs::PermissionsExt;

    let project = static_project();
    let state = project.path().join(".anybuild");
    let archive = state.join("runner/lambda/function.zip");
    std::fs::create_dir_all(archive.parent().unwrap()).unwrap();
    std::fs::write(&archive, "zip-placeholder").unwrap();
    std::fs::write(
        state.join("artifact.json"),
        serde_json::to_string_pretty(&RuntimeArtifact::LambdaZip {
            archive,
            runtime: "python3.13".to_owned(),
            handler: "run.sh".to_owned(),
            environment: indexmap::IndexMap::from([
                (
                    "AWS_LAMBDA_EXEC_WRAPPER".to_owned(),
                    "/opt/bootstrap".to_owned(),
                ),
                ("AWS_LWA_PORT".to_owned(), "8080".to_owned()),
            ]),
            platform: Some("linux/amd64".to_owned()),
        })
        .unwrap(),
    )
    .unwrap();

    let command_log = project.path().join("commands.log");
    let function_marker = project.path().join("function-created");
    let fake_aws = project.path().join("fake-aws");
    std::fs::write(
        &fake_aws,
        r#"#!/bin/sh
printf 'aws %s\n' "$*" >> "$COMMAND_LOG"
case "$1 $2" in
  'lambda get-function')
    if [ ! -f "$FUNCTION_MARKER" ]; then
      echo 'ResourceNotFoundException' >&2
      exit 254
    fi
    echo '{"Configuration":{"PackageType":"Zip","Environment":{"Variables":{"KEEP":"yes"}},"Layers":[{"Arn":"arn:aws:lambda:us-west-2:123456789012:layer:observability:1"}]}}'
    ;;
  'lambda create-function') touch "$FUNCTION_MARKER" ;;
  'lambda update-function-code') ;;
  'lambda update-function-configuration') ;;
  'lambda wait') ;;
  *) echo "unexpected AWS command: $*" >&2; exit 1 ;;
esac
"#,
    )
    .unwrap();
    std::fs::set_permissions(&fake_aws, std::fs::Permissions::from_mode(0o755)).unwrap();

    let sdk = Anybuild::new(project.path())
        .with_env("COMMAND_LOG", command_log.display().to_string())
        .with_env("FUNCTION_MARKER", function_marker.display().to_string());
    let options = |role: Option<&str>| DeployOptions {
        platform: DeploymentPlatform::AwsLambda(AwsLambdaOptions {
            binary: Some(fake_aws.display().to_string()),
            region: Some("us-west-2".to_owned()),
            function: Some("sdk-lambda".to_owned()),
            role: role.map(str::to_owned),
            ..Default::default()
        }),
        target: DeployTarget::Publish {
            owner: None,
            name: None,
        },
        process_io: ProcessIo::Events,
    };

    sdk.deploy(options(Some(
        "arn:aws:iam::123456789012:role/lambda-execution",
    )))
    .unwrap();
    sdk.deploy(options(None)).unwrap();

    let log = std::fs::read_to_string(command_log).unwrap();
    assert!(!log.contains(" ecr "));
    assert!(!log.lines().any(|line| line.starts_with("docker ")));
    assert!(log.contains("lambda create-function"));
    assert!(log.contains("--package-type Zip"));
    assert!(log.contains("--runtime python3.13"));
    assert!(log.contains("--handler run.sh"));
    assert!(log.contains("--zip-file fileb://"));
    assert!(log.contains("arn:aws:lambda:us-west-2:753240598075:layer:LambdaAdapterLayerX86:28"));
    assert!(log.contains("lambda update-function-code"));
    assert!(log.contains("lambda update-function-configuration"));
    assert!(log.contains("lambda wait function-updated-v2"));
    assert!(log.contains("KEEP"));
    assert!(log.contains("observability:1"));
}

#[test]
fn deployment_rejects_an_incompatible_runtime_artifact() {
    let project = static_project();
    let state = project.path().join(".anybuild");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(
        state.join("artifact.json"),
        format!(
            "{{\"kind\":\"local\",\"directory\":{}}}\n",
            serde_json::to_string(&state.join("runner/local")).unwrap()
        ),
    )
    .unwrap();

    let error = Anybuild::new(project.path())
        .deploy(DeployOptions {
            platform: DeploymentPlatform::default(),
            target: DeployTarget::Publish {
                owner: None,
                name: None,
            },
            process_io: ProcessIo::Events,
        })
        .unwrap_err();

    let message = error.to_string();
    assert!(message.contains("Wasmer deployment requires a Wasmer artifact"));
    assert!(message.contains("found Local"));

    let error = Anybuild::new(project.path())
        .deploy(DeployOptions {
            platform: DeploymentPlatform::Fly(FlyOptions::default()),
            target: DeployTarget::Publish {
                owner: None,
                name: None,
            },
            process_io: ProcessIo::Events,
        })
        .unwrap_err();

    let message = error.to_string();
    assert!(message.contains("Fly.io deployment requires a Docker artifact"));
    assert!(message.contains("found Local"));

    let error = Anybuild::new(project.path())
        .deploy(DeployOptions {
            platform: DeploymentPlatform::AwsLambda(AwsLambdaOptions::default()),
            target: DeployTarget::Publish {
                owner: None,
                name: None,
            },
            process_io: ProcessIo::Events,
        })
        .unwrap_err();

    let message = error.to_string();
    assert!(message.contains("AWS Lambda deployment requires a Lambda ZIP or Docker artifact"));
    assert!(message.contains("found Local"));
}
