//! `anybuild plan` (port of cli.py's plan command).

use std::path::PathBuf;

use anybuild_plan::Step;
use anyhow::{bail, Result};

use crate::commands::generate;
use crate::context::{resolve_project_context, CommandOverrides, EnvironmentOptions};
use crate::paths::{migrate_legacy_anybuild, resolve_project_paths};
use crate::SharedProjectArgs;

pub fn run(
    shared: SharedProjectArgs,
    out: Option<PathBuf>,
    mut anybuild_path: Option<PathBuf>,
    mut regenerate: bool,
    temp_anybuild: bool,
    serve_port: Option<i64>,
    env_options: EnvironmentOptions,
) -> Result<()> {
    let paths = resolve_project_paths(&shared.path, shared.subdir.as_deref())?;

    let _temp_file;
    if temp_anybuild {
        if anybuild_path.is_some() {
            bail!("Cannot use both --temp-anybuild and --anybuild-path");
        }
        let file = tempfile::Builder::new().prefix("Anybuild").tempfile()?;
        anybuild_path = Some(file.path().to_path_buf());
        _temp_file = file;
    }

    if !regenerate {
        match &anybuild_path {
            Some(path) if !path.exists() => regenerate = true,
            None if migrate_legacy_anybuild(&paths)?.is_none() => regenerate = true,
            _ => {}
        }
    }
    if regenerate || temp_anybuild {
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
            anybuild_path.clone(),
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
        anybuild_path.as_deref(),
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
        "config": anybuild_providers::exclude_defaults_json(&provider_config),
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

/// `json.dumps(..., indent=4)` formatting.
fn to_json_indent4(value: &serde_json::Value) -> Result<String> {
    let mut buf = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
    serde::Serialize::serialize(value, &mut ser)?;
    Ok(String::from_utf8(buf)?)
}
