use std::path::PathBuf;

use anybuild::{PlanOptions, RuntimeEnvironment};
use anyhow::Result;

use crate::commands::{client, execution};
use crate::context::EnvironmentOptions;
use crate::SharedProjectArgs;

pub fn run(
    shared: SharedProjectArgs,
    out: Option<PathBuf>,
    anybuild_path: Option<PathBuf>,
    regenerate: bool,
    temp_anybuild: bool,
    serve_port: Option<i64>,
    environment: EnvironmentOptions,
) -> Result<()> {
    let (build_environment, mut runtime_environment) = execution(
        environment.wasmer,
        environment.wasmer_bin,
        environment.wasmer_registry,
        environment.wasmer_token,
        environment.docker,
        environment.docker_client,
        environment.docker_opts,
    );
    if !environment.wasmer {
        runtime_environment = RuntimeEnvironment::Local;
    }
    let plan = client(&shared, serve_port)?.plan(PlanOptions {
        anybuild_path,
        regenerate,
        temporary: temp_anybuild,
        serve_port,
        build_environment,
        runtime_environment,
        ..Default::default()
    })?;
    let output = serde_json::json!({
        "provider": plan.provider,
        "config": plan.config,
        "services": plan.services
            .iter()
            .map(|service| serde_json::json!({
                "name": service.name,
                "provider": service.provider,
            }))
            .collect::<Vec<_>>(),
    });
    let mut bytes = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut serializer = serde_json::Serializer::with_formatter(&mut bytes, formatter);
    serde::Serialize::serialize(&output, &mut serializer)?;
    let json = String::from_utf8(bytes)?;
    if let Some(path) = out {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, json)?;
        println!(
            "Plan saved to {}",
            path.canonicalize().unwrap_or(path).display()
        );
    } else {
        println!("{json}");
    }
    Ok(())
}
