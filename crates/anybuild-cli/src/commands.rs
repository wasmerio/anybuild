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

use crate::SharedProjectArgs;

pub(crate) fn client(shared: &SharedProjectArgs, serve_port: Option<i64>) -> Result<Anybuild> {
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
        .with_event_handler(render_event);
    if let Some(subdir) = &shared.subdir {
        client = client.with_subdir(subdir);
    }
    Ok(client)
}

pub(crate) fn execution(
    wasmer: bool,
    wasmer_bin: Option<String>,
    wasmer_registry: Option<String>,
    wasmer_token: Option<String>,
    docker: bool,
    docker_client: Option<String>,
    docker_opts: Option<String>,
) -> (BuildEnvironment, RuntimeEnvironment) {
    let build = if docker || docker_client.is_some() {
        BuildEnvironment::Docker(DockerOptions {
            client: docker_client,
            extra_options: docker_opts,
        })
    } else {
        BuildEnvironment::Local
    };
    let runtime = if wasmer {
        RuntimeEnvironment::Wasmer(WasmerOptions {
            binary: wasmer_bin,
            registry: wasmer_registry,
            token: wasmer_token,
        })
    } else {
        RuntimeEnvironment::Local
    };
    (build, runtime)
}

fn render_event(event: &Event) {
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
            eprintln!("Detected {} provider", provider_display_name(provider));
            for detail in details {
                eprintln!("  {}: {}", detail.label, detail.value);
            }
        }
        Event::FileWritten { kind, path } => {
            eprintln!("Generated {kind} at {}", path.display())
        }
        Event::BuildStep { description } => eprintln!("{description}"),
        Event::CommandStarted { name, .. } => eprintln!("\nRunning command {name}"),
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
