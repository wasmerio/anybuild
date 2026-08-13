pub mod auto;
pub mod build;
pub mod deploy;
pub mod generate;
pub mod plan;
pub mod run;

use anybuild::{
    Anybuild, BuildEnvironment, CommandOverrides, DiagnosticLevel, DockerOptions, Event,
    RuntimeEnvironment, WasmerOptions,
};
use anyhow::{Context, Result};

use crate::args::{BuildTarget, ExecutionTargetArgs, RunTarget};
use crate::context::EnvironmentOptions;
use crate::SharedProjectArgs;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RenderOptions {
    pub show_detailed_steps: bool,
    pub show_wasmer_files: bool,
}

pub(crate) fn client(shared: &SharedProjectArgs, serve_port: Option<i64>) -> Result<Anybuild> {
    client_with_render_options(shared, serve_port, RenderOptions::default())
}

pub(crate) fn client_with_render_options(
    shared: &SharedProjectArgs,
    serve_port: Option<i64>,
    render_options: RenderOptions,
) -> Result<Anybuild> {
    let config = shared
        .config
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .context("--config must be valid JSON")?;
    let mut client = Anybuild::new(&shared.path)
        .with_command_overrides(CommandOverrides {
            start_command: shared.start_command.clone(),
            install_command: shared.install_command.clone(),
            build_command: shared.build_command.clone(),
            serve_port,
            use_provider: shared.provider.clone(),
            config,
        })
        .with_event_handler(move |event: &Event| render_event(event, render_options));
    if let Some(subdir) = &shared.subdir {
        client = client.with_subdir(subdir);
    }
    Ok(client)
}

pub(crate) fn execution(
    targets: ExecutionTargetArgs,
    environment: EnvironmentOptions,
) -> Result<(BuildEnvironment, RuntimeEnvironment)> {
    if targets.builder == Some(BuildTarget::Local)
        && (environment.docker
            || (environment.docker_client.is_some() && targets.runner != Some(RunTarget::Docker)))
    {
        anyhow::bail!("--builder=local cannot be combined with --docker or --docker-client");
    }
    if targets.runner == Some(RunTarget::Local) && environment.wasmer {
        anyhow::bail!("--runner=local cannot be combined with --wasmer");
    }
    if targets.runner == Some(RunTarget::Docker) && environment.wasmer {
        anyhow::bail!("--runner=docker cannot be combined with --wasmer");
    }

    let docker_client_selects_builder = environment.docker_client.is_some()
        && targets.builder.is_none()
        && targets.runner != Some(RunTarget::Docker);
    let build = if targets.builder == Some(BuildTarget::Docker)
        || environment.docker
        || docker_client_selects_builder
    {
        BuildEnvironment::Docker(DockerOptions {
            client: environment.docker_client.clone(),
            extra_options: environment.docker_opts.clone(),
        })
    } else {
        BuildEnvironment::Local
    };
    let runtime = if targets.runner == Some(RunTarget::Wasmer) || environment.wasmer {
        RuntimeEnvironment::Wasmer(WasmerOptions {
            binary: environment.wasmer_bin,
            registry: environment.wasmer_registry,
            token: environment.wasmer_token,
        })
    } else if targets.runner == Some(RunTarget::Docker) {
        RuntimeEnvironment::Docker(DockerOptions {
            client: environment.docker_client,
            extra_options: environment.docker_opts,
        })
    } else {
        RuntimeEnvironment::Local
    };
    Ok((build, runtime))
}

fn render_event(event: &Event, options: RenderOptions) {
    match event {
        Event::Diagnostic { level, message } => match level {
            DiagnosticLevel::Info => eprintln!("{message}"),
            DiagnosticLevel::Warning => eprintln!("Warning: {message}"),
        },
        Event::LegacyRenamed { from, to } => {
            let from = from
                .file_name()
                .unwrap_or(from.as_os_str())
                .to_string_lossy();
            let to = to.file_name().unwrap_or(to.as_os_str()).to_string_lossy();
            if from.starts_with('.') && to.starts_with('.') {
                eprintln!("Renamed legacy {from} directory to {to}.")
            } else {
                eprintln!("Renamed legacy {from} file to {to}.")
            }
        }
        Event::ProviderDetected { provider, details } => {
            eprint!(
                "{}",
                crate::render::render_provider_status(
                    "Detected",
                    provider_display_name(provider),
                    "provider",
                    crate::render::colors_enabled(),
                )
            );
            for detail in details {
                eprintln!("    {}: {}", detail.label, detail.value);
            }
        }
        Event::ProviderDeclared { provider, details } => {
            eprint!(
                "{}",
                crate::render::render_provider_status(
                    "Using",
                    provider_display_name(provider),
                    "provider declared in Anybuild",
                    crate::render::colors_enabled(),
                )
            );
            for detail in details {
                eprintln!("    {}: {}", detail.label, detail.value);
            }
        }
        Event::AnybuildGenerating { .. } => {}
        Event::BuildPlan {
            packages,
            steps,
            prepare_steps,
            deploy_scripts,
        } => {
            eprintln!();
            eprint!(
                "{}",
                crate::render::render_build_plan(
                    packages,
                    steps,
                    prepare_steps,
                    deploy_scripts,
                    options.show_detailed_steps,
                    crate::render::colors_enabled(),
                )
            )
        }
        Event::FileWritten {
            kind: "anybuild",
            path,
        } => eprintln!("\n  Generated Anybuild at {}", path.display()),
        Event::FileWritten { kind, path } => eprintln!("Generated {kind} at {}", path.display()),
        Event::SectionStarted { title } => {
            eprintln!();
            eprint!(
                "{}",
                crate::render::render_section_header(title, crate::render::colors_enabled())
            )
        }
        Event::WasmerPackageMappings { mappings } => eprint!(
            "{}",
            crate::render::render_wasmer_package_mappings(
                mappings,
                crate::render::colors_enabled()
            )
        ),
        Event::Success { message } => eprint!(
            "{}",
            crate::render::render_success(message, crate::render::colors_enabled())
        ),
        Event::WasmerFileContent {
            filename,
            content,
            language,
        } if options.show_wasmer_files => {
            eprint!(
                "{}",
                crate::render::render_success(
                    &format!("Created {filename} manifest"),
                    crate::render::colors_enabled(),
                )
            );
            eprint!(
                "{}",
                crate::render::render_panel(
                    content,
                    Some(language),
                    crate::render::colors_enabled(),
                )
            )
        }
        Event::BuildStarted => eprint!(
            "{}",
            crate::render::render_section_header(
                "Starting Build...",
                crate::render::colors_enabled(),
            )
        ),
        Event::BuildStep { description } => eprint!(
            "{}",
            crate::render::render_build_progress(description, crate::render::colors_enabled(),)
        ),
        Event::CommandStarted { name, .. } => {
            eprintln!();
            eprint!(
                "{}",
                crate::render::render_section_header(
                    &format!("Run {name} command"),
                    crate::render::colors_enabled(),
                )
            )
        }
        Event::ProcessOutput { stream: _, text } => eprint!("{text}"),
        Event::Content { content, language } => eprint!(
            "{}",
            crate::render::render_panel(
                content,
                language.as_deref(),
                crate::render::colors_enabled(),
            )
        ),
        Event::ArtifactCreated { .. } => {}
        Event::Deployment { description } => println!("{description}"),
        _ => {}
    }
}

fn provider_display_name(provider: &str) -> &str {
    match provider {
        "node" => "Node.js",
        "node-static" => "static Node.js",
        "staticfile" => "static site",
        "wordpress" => "WordPress",
        "mkdocs" => "MkDocs",
        "php" => "PHP",
        "go" => "Go",
        "hugo" => "Hugo",
        "jekyll" => "Jekyll",
        "laravel" => "Laravel",
        "python" => "Python",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn targets(builder: Option<BuildTarget>, runner: Option<RunTarget>) -> ExecutionTargetArgs {
        ExecutionTargetArgs { builder, runner }
    }

    #[test]
    fn explicit_execution_targets_select_the_requested_environments() {
        let (build, runtime) = execution(
            targets(Some(BuildTarget::Docker), Some(RunTarget::Wasmer)),
            EnvironmentOptions::default(),
        )
        .unwrap();

        assert!(matches!(build, BuildEnvironment::Docker(_)));
        assert!(matches!(runtime, RuntimeEnvironment::Wasmer(_)));
    }

    #[test]
    fn docker_runner_is_independent_from_the_builder() {
        let (build, runtime) = execution(
            targets(Some(BuildTarget::Local), Some(RunTarget::Docker)),
            EnvironmentOptions {
                docker_client: Some("podman".to_owned()),
                ..Default::default()
            },
        )
        .unwrap();

        assert!(matches!(build, BuildEnvironment::Local));
        assert!(matches!(
            runtime,
            RuntimeEnvironment::Docker(DockerOptions {
                client: Some(client),
                ..
            }) if client == "podman"
        ));
    }

    #[test]
    fn legacy_execution_flags_remain_shorthands() {
        let (build, runtime) = execution(
            ExecutionTargetArgs::default(),
            EnvironmentOptions {
                wasmer: true,
                docker: true,
                ..Default::default()
            },
        )
        .unwrap();

        assert!(matches!(build, BuildEnvironment::Docker(_)));
        assert!(matches!(runtime, RuntimeEnvironment::Wasmer(_)));
    }

    #[test]
    fn contradictory_execution_targets_are_rejected() {
        assert!(execution(
            targets(Some(BuildTarget::Local), None),
            EnvironmentOptions {
                docker: true,
                ..Default::default()
            },
        )
        .is_err());
        assert!(execution(
            targets(None, Some(RunTarget::Local)),
            EnvironmentOptions {
                wasmer: true,
                ..Default::default()
            },
        )
        .is_err());
    }
}
