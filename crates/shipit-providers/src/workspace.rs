//! Subdir workspace config application (port of `cli.py`'s
//! `apply_subdir_provider_config` / `apply_subdir_workspace_config`).

use std::path::Path;

use crate::base::CustomCommands;
use crate::node::{detect_package_manager, PackageManager};
use crate::ProviderConfig;

/// Port of `apply_subdir_provider_config`: every provider config has
/// `app_subdir`, and cli.py assigns it unconditionally (clearing any
/// env-provided value when there is no subdir).
pub fn apply_subdir_provider_config(config: &mut ProviderConfig, subdir: Option<&str>) {
    config.base_mut().app_subdir = subdir.map(str::to_owned);
}

/// Port of `apply_subdir_workspace_config`: subdir node apps without
/// their own lockfile inherit the workspace root's package manager, and
/// `<pm> run …` build commands are rewritten to match.
pub fn apply_subdir_workspace_config(workspace_root: &Path, config: &mut ProviderConfig) {
    // cli.py takes the subdir from ProjectPaths; by the time this runs
    // apply_subdir_provider_config has recorded it on the config.
    let Some(subdir) = config
        .base()
        .app_subdir
        .clone()
        .filter(|subdir| !subdir.is_empty())
    else {
        return;
    };

    // `hasattr(provider_config, "package_manager")`: node / node-static.
    match config {
        ProviderConfig::Node(node) => apply_node_workspace_config(
            workspace_root,
            &subdir,
            &mut node.package_manager,
            &mut node.build_command,
            &mut node.base.commands,
        ),
        ProviderConfig::NodeStatic(node_static) => apply_node_workspace_config(
            workspace_root,
            &subdir,
            &mut node_static.package_manager,
            &mut node_static.build_command,
            &mut node_static.base.commands,
        ),
        _ => {}
    }
}

fn apply_node_workspace_config(
    workspace_root: &Path,
    subdir: &str,
    package_manager: &mut Option<PackageManager>,
    build_command: &mut Option<String>,
    commands: &mut CustomCommands,
) {
    let app_path = workspace_root.join(subdir);
    if !app_path.join("package.json").exists() {
        return;
    }

    let app_has_lockfile = PackageManager::ALL
        .iter()
        .any(|manager| app_path.join(manager.lockfile()).exists());
    if app_has_lockfile {
        return;
    }

    let workspace_manager = detect_package_manager(workspace_root);
    let current_manager = *package_manager;
    if current_manager == Some(workspace_manager) {
        return;
    }

    *package_manager = Some(workspace_manager);
    // load_config always sets package_manager; a None here would raise in
    // Python's _rewrite_package_manager_command for non-empty commands.
    if let Some(current_manager) = current_manager {
        *build_command = rewrite_package_manager_command(
            build_command.take(),
            current_manager,
            workspace_manager,
        );
        // `if provider_config.commands:` — pydantic models are always
        // truthy, so the rewrite always runs.
        commands.build = rewrite_package_manager_command(
            commands.build.take(),
            current_manager,
            workspace_manager,
        );
    }
}

/// Port of cli.py's `_rewrite_package_manager_command`.
fn rewrite_package_manager_command(
    command: Option<String>,
    old_manager: PackageManager,
    new_manager: PackageManager,
) -> Option<String> {
    let command = command?;
    if command.is_empty() {
        return Some(command); // Python: falsy commands returned unchanged
    }
    let old_run = format!("{} run ", old_manager.value());
    if let Some(rest) = command.strip_prefix(&old_run) {
        return Some(format!("{} run {rest}", new_manager.value()));
    }
    Some(command)
}
