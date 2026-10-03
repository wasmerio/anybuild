//! Subdirectory workspace config application.

use std::path::Path;

use crate::providers::node::PackageManager;
use crate::providers::ProviderConfig;

/// Port of `apply_subdir_provider_config`: every provider config has
/// `app_subdir`, and cli.py assigns it unconditionally (clearing any
/// env-provided value when there is no subdir).
pub fn apply_subdir_provider_config(config: &mut ProviderConfig, subdir: Option<&str>) {
    config.base_mut().app_subdir = subdir.map(str::to_owned);
}

/// Subdirectory apps without a lockfile inherit the workspace toolchain.
pub(crate) fn node_package_manager_path<'a>(app: &'a Path, subdir: Option<&str>) -> &'a Path {
    let Some(subdir) = subdir.filter(|subdir| !subdir.is_empty()) else {
        return app;
    };
    if !app.join("package.json").exists()
        || PackageManager::ALL
            .iter()
            .any(|manager| manager.has_lockfile(app))
    {
        return app;
    }
    app.ancestors()
        .nth(Path::new(subdir).components().count())
        .unwrap_or(app)
}
