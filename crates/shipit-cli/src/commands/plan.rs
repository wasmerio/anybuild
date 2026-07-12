//! `shipit plan` (port of cli.py's plan command).

use std::path::PathBuf;

use anyhow::{bail, Result};
use shipit_plan::Step;

use crate::commands::generate;
use crate::context::{resolve_project_context, CommandOverrides, EnvironmentOptions};
use crate::paths::{default_shipit_path, resolve_project_paths};
use crate::SharedProjectArgs;

pub fn run(
    shared: SharedProjectArgs,
    out: Option<PathBuf>,
    mut shipit_path: Option<PathBuf>,
    mut regenerate: bool,
    temp_shipit: bool,
    serve_port: Option<i64>,
    env_options: EnvironmentOptions,
) -> Result<()> {
    let paths = resolve_project_paths(&shared.path, shared.subdir.as_deref())?;

    let _temp_file;
    if temp_shipit {
        if shipit_path.is_some() {
            bail!("Cannot use both --temp-shipit and --shipit-path");
        }
        let file = tempfile::Builder::new().prefix("Shipit").tempfile()?;
        shipit_path = Some(file.path().to_path_buf());
        _temp_file = file;
    }

    if !regenerate {
        match &shipit_path {
            Some(path) if !path.exists() => regenerate = true,
            None if !default_shipit_path(&paths).exists() => regenerate = true,
            _ => {}
        }
    }
    if regenerate || temp_shipit {
        generate::run(
            SharedProjectArgs {
                path: paths.workspace_root.clone(),
                subdir: paths.subdir.clone(),
                install_command: shared.install_command.clone(),
                build_command: shared.build_command.clone(),
                start_command: shared.start_command.clone(),
                provider: shared.provider.clone(),
                config: shared.config.clone(),
            },
            shipit_path.clone(),
        )?;
    }

    let overrides = CommandOverrides {
        start_command: shared.start_command,
        install_command: shared.install_command,
        build_command: shared.build_command,
        serve_port,
        use_provider: shared.provider,
        config: shared.config,
    };
    let context = resolve_project_context(
        &paths.workspace_root,
        paths.subdir.as_deref(),
        shipit_path.as_deref(),
        &overrides,
        &env_options,
    )?;
    let serve = &context.serve;
    let mut provider_config = context.provider_config.clone();

    let collect_group = |group: &str| -> Option<String> {
        let commands: Vec<&str> = serve
            .build
            .iter()
            .filter_map(|step| match step {
                Step::Run(run) if run.group.as_deref() == Some(group) => Some(run.command.as_str()),
                _ => None,
            })
            .collect();
        if commands.is_empty() {
            None
        } else {
            Some(commands.join(" && "))
        }
    };

    if let Some(start) = serve.commands.get("start") {
        provider_config.base_mut().commands.start = Some(start.clone());
    }
    if let Some(after_deploy) = serve.commands.get("after_deploy") {
        provider_config.base_mut().commands.after_deploy = Some(after_deploy.clone());
    }
    if let Some(install) = collect_group("install") {
        provider_config.base_mut().commands.install = Some(install);
    }
    if let Some(build) = collect_group("build") {
        provider_config.base_mut().commands.build = Some(build);
    }

    let plan_output = serde_json::json!({
        "provider": context.provider,
        "config": exclude_defaults_json(&provider_config),
        "services": serve
            .services
            .iter()
            .flatten()
            .map(|svc| serde_json::json!({"name": svc.name, "provider": svc.provider}))
            .collect::<Vec<_>>(),
    });
    let json_output = to_json_indent4(&plan_output)?;
    match out {
        Some(out) => {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&out, &json_output)?;
            println!(
                "Plan saved to {}",
                out.canonicalize().unwrap_or(out).display()
            );
        }
        None => {
            println!("{json_output}");
        }
    }
    Ok(())
}

/// pydantic `model_dump_json(exclude_defaults=True)`: drop fields equal to
/// their declared defaults, recursively for nested models.
pub(crate) fn exclude_defaults_json(
    config: &shipit_providers::ProviderConfig,
) -> serde_json::Value {
    let dumped = config.to_json();
    let defaults = shipit_providers::base::without_env(|| defaults_for(config));
    match (dumped, defaults) {
        (serde_json::Value::Object(dumped), Some(serde_json::Value::Object(defaults))) => {
            exclude_object(dumped, &defaults)
        }
        (dumped, _) => dumped,
    }
}

fn defaults_for(config: &shipit_providers::ProviderConfig) -> Option<serde_json::Value> {
    shipit_providers::defaults_json(config.provider_name()).ok()
}

fn exclude_object(
    dumped: serde_json::Map<String, serde_json::Value>,
    defaults: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    for (key, value) in dumped {
        match defaults.get(&key) {
            Some(default) if *default == value => {}
            Some(serde_json::Value::Object(default_child)) => {
                if let serde_json::Value::Object(child) = value {
                    let reduced = exclude_object(child, default_child);
                    if reduced.as_object().map(|m| !m.is_empty()).unwrap_or(true) {
                        out.insert(key, reduced);
                    }
                } else {
                    out.insert(key, value);
                }
            }
            _ => {
                out.insert(key, value);
            }
        }
    }
    serde_json::Value::Object(out)
}

/// `json.dumps(..., indent=4)` formatting.
fn to_json_indent4(value: &serde_json::Value) -> Result<String> {
    let mut buf = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
    serde::Serialize::serialize(value, &mut ser)?;
    Ok(String::from_utf8(buf)?)
}
