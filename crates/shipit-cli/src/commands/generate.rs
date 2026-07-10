//! `shipit generate` (port of cli.py's generate command).

use std::path::PathBuf;

use anyhow::Result;

use crate::context::{load_project_config, CommandOverrides};
use crate::generator::generate_shipit;
use crate::paths::{default_shipit_path, resolve_project_paths};
use crate::SharedProjectArgs;

pub fn run(shared: SharedProjectArgs, out: Option<PathBuf>) -> Result<()> {
    let paths = resolve_project_paths(&shared.path, shared.subdir.as_deref())?;
    let out = out.unwrap_or_else(|| default_shipit_path(&paths));

    let overrides = CommandOverrides {
        start_command: shared.start_command,
        install_command: shared.install_command,
        build_command: shared.build_command,
        serve_port: None,
        use_provider: shared.provider,
        config: shared.config,
    };
    let (provider, provider_config) = load_project_config(&paths, &overrides)?;

    let content = generate_shipit(
        provider,
        provider_config.base().name.as_deref(),
        paths.subdir.as_deref(),
    )?;
    // Python: provider_config.model_dump_json(indent=2, exclude_defaults=True)
    // rendered in a line-numbered panel when non-empty.
    let config_json = crate::commands::plan::exclude_defaults_json(&provider_config);
    let config_json = serde_json::to_string_pretty(&config_json)?;
    if !config_json.is_empty() && config_json != "{}" {
        shipit_build::local::ui::print_panel(&config_json);
    }
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&out, &content)?;
    // console.print(...) -> stderr, like the Python CLI.
    eprintln!(
        "Generated Shipit at {}",
        out.canonicalize().unwrap_or(out).display()
    );
    Ok(())
}
