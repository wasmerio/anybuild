//! Workspace inheritance and Node package-manager overrides.

use std::path::Path;

use crate::providers::base::CustomCommands;
use crate::providers::node::{
    detect_package_manager, resolve_manager_version, resolve_workspace_manager_version,
    NodeBuildConfigFields, PackageManager,
};
use crate::providers::ProviderConfig;

/// Port of `apply_subdir_provider_config`: every provider config has
/// `app_subdir`, and cli.py assigns it unconditionally (clearing any
/// env-provided value when there is no subdir).
pub fn apply_subdir_provider_config(config: &mut ProviderConfig, subdir: Option<&str>) {
    config.base_mut().app_subdir = subdir.map(str::to_owned);
}

/// Subdirectory Node apps without their own lockfile inherit the workspace
/// root's package manager, and `<pm> run …` commands are rewritten to match.
pub(crate) fn apply_node_workspace_config(
    workspace_root: &Path,
    subdir: Option<&str>,
    build: &mut NodeBuildConfigFields,
    commands: &mut CustomCommands,
) {
    let Some(subdir) = subdir.filter(|subdir| !subdir.is_empty()) else {
        return;
    };
    let app_path = workspace_root.join(subdir);
    if !app_path.join("package.json").exists() {
        return;
    }

    let app_has_lockfile = PackageManager::ALL
        .iter()
        .any(|manager| manager.has_lockfile(&app_path));
    if app_has_lockfile {
        return;
    }

    let workspace_manager = detect_package_manager(workspace_root);
    let current_manager = build.package_manager;
    // The root declares the toolchain for the whole workspace, so it gets a say
    // on the version even when it does not change the manager itself.
    resolve_workspace_manager_version(build, workspace_manager, current_manager, workspace_root);
    if current_manager == Some(workspace_manager) {
        return;
    }

    build.package_manager = Some(workspace_manager);
    // load_config always sets package_manager; a None here would raise in
    // Python's _rewrite_package_manager_command for non-empty commands.
    if let Some(current_manager) = current_manager {
        build.build_command = rewrite_package_manager_command(
            build.build_command.take(),
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

/// Persisted build commands still reference the manager used at generation.
/// Reconcile them after runtime overrides, without replacing explicit commands.
pub(crate) fn apply_package_manager_override(
    config: &mut ProviderConfig,
    previous: &ProviderConfig,
    app_path: &Path,
) {
    let (build, previous_build, commands, previous_commands) = match (config, previous) {
        (ProviderConfig::Node(config), ProviderConfig::Node(previous)) => (
            &mut config.node.build,
            &previous.node.build,
            &mut config.base.commands,
            &previous.base.commands,
        ),
        (ProviderConfig::NodeStatic(config), ProviderConfig::NodeStatic(previous)) => (
            &mut config.node.build,
            &previous.node.build,
            &mut config.base.commands,
            &previous.base.commands,
        ),
        (ProviderConfig::Laravel(config), ProviderConfig::Laravel(previous)) => (
            &mut config.node,
            &previous.node,
            &mut config.base.commands,
            &previous.base.commands,
        ),
        _ => return,
    };
    let (Some(old_manager), Some(new_manager)) =
        (previous_build.package_manager, build.package_manager)
    else {
        return;
    };
    if old_manager == new_manager {
        return;
    }

    resolve_manager_version(build, new_manager, app_path);
    if build.build_command == previous_build.build_command {
        build.build_command =
            rewrite_package_manager_command(build.build_command.take(), old_manager, new_manager);
    }
    if commands.build == previous_commands.build {
        commands.build =
            rewrite_package_manager_command(commands.build.take(), old_manager, new_manager);
    }
}

fn rewrite_package_manager_command(
    command: Option<String>,
    old_manager: PackageManager,
    new_manager: PackageManager,
) -> Option<String> {
    let command = command?;
    Some(rewrite_command(&command, old_manager, new_manager).unwrap_or(command))
}

fn rewrite_command(
    command: &str,
    old_manager: PackageManager,
    new_manager: PackageManager,
) -> Option<String> {
    // Next.js wraps a quoted script command in a second package-manager call.
    // Parse that argument before rewriting so its shell quoting stays intact.
    if let Some(rest) = command.strip_prefix(&old_manager.dlx_command("next-bundle@")) {
        let (version, quoted) = rest.split_once(" --build-command ")?;
        let args = shlex::split(quoted)?;
        let [inner] = args.as_slice() else {
            return None;
        };
        let inner =
            rewrite_command(inner, old_manager, new_manager).unwrap_or_else(|| inner.to_owned());
        let quoted = shlex::try_quote(&inner).ok()?;
        return Some(format!(
            "{} --build-command {quoted}",
            new_manager.dlx_command(&format!("next-bundle@{version}"))
        ));
    }
    for (old_prefix, new_prefix) in [
        (old_manager.run_command(""), new_manager.run_command("")),
        (old_manager.dlx_command(""), new_manager.dlx_command("")),
        (
            old_manager.run_execute_command(""),
            new_manager.run_execute_command(""),
        ),
    ] {
        if let Some(rest) = command.strip_prefix(&old_prefix) {
            return Some(format!("{new_prefix}{rest}"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_bundle_rewrite_preserves_quoted_script_arguments() {
        let command = r#"pnpm dlx next-bundle@1.0.0 --build-command "pnpm run build -- --label 'hello world'""#;
        let rewritten = rewrite_command(command, PackageManager::Pnpm, PackageManager::Npm)
            .expect("Next.js command is rewritten");
        assert_eq!(
            shlex::split(&rewritten).unwrap(),
            [
                "npx",
                "-y",
                "next-bundle@1.0.0",
                "--build-command",
                "npm run build -- --label 'hello world'",
            ]
        );
    }
}
