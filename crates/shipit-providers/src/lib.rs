//! Shipit providers: project detection and config loading.
//!
//! Port of `src/shipit/providers/` + the dispatch in `generator.py`.
//! Each provider module exposes `detect(path, base)` and
//! `load_config(path, base)`; the registry order matches Python's
//! (more specific providers first, ties broken by order).

use std::path::Path;

use anyhow::{anyhow, Result};

pub mod base;
pub mod go;
pub mod hugo;
pub mod install_context;
pub mod jekyll;
pub mod laravel;
pub mod mkdocs;
pub mod node;
pub mod node_static;
pub mod php;
pub mod procfile;
pub mod python;
pub mod staticfile;
pub mod wordpress;
pub mod workspace;

pub use base::{BaseConfig, CustomCommands, DetectResult, HasBase};

type DetectFn = fn(&Path, &BaseConfig) -> Option<DetectResult>;

/// Order matters: more specific providers first (mirror of registry.py).
pub const REGISTRY: &[(&str, DetectFn)] = &[
    ("laravel", laravel::detect),
    ("hugo", hugo::detect),
    ("mkdocs", mkdocs::detect),
    ("python", python::detect),
    ("wordpress", wordpress::detect),
    ("php", php::detect),
    ("node-static", node_static::detect),
    ("node", node::detect),
    ("jekyll", jekyll::detect),
    ("go", go::detect),
    ("staticfile", staticfile::detect),
];

/// Every provider's loaded config.
#[derive(Debug, Clone)]
pub enum ProviderConfig {
    Python(python::PythonConfig),
    Node(node::NodeConfig),
    NodeStatic(node_static::NodeStaticConfig),
    Php(php::PhpConfig),
    Wordpress(wordpress::WordPressConfig),
    Laravel(laravel::LaravelConfig),
    Go(go::GoConfig),
    StaticFile(staticfile::StaticFileConfig),
    Hugo(hugo::HugoConfig),
    Jekyll(jekyll::JekyllConfig),
    Mkdocs(mkdocs::MkdocsConfig),
}

macro_rules! each_config {
    ($self:expr, $config:ident => $body:expr) => {
        match $self {
            ProviderConfig::Python($config) => $body,
            ProviderConfig::Node($config) => $body,
            ProviderConfig::NodeStatic($config) => $body,
            ProviderConfig::Php($config) => $body,
            ProviderConfig::Wordpress($config) => $body,
            ProviderConfig::Laravel($config) => $body,
            ProviderConfig::Go($config) => $body,
            ProviderConfig::StaticFile($config) => $body,
            ProviderConfig::Hugo($config) => $body,
            ProviderConfig::Jekyll($config) => $body,
            ProviderConfig::Mkdocs($config) => $body,
        }
    };
}

impl ProviderConfig {
    pub fn provider_name(&self) -> &'static str {
        match self {
            ProviderConfig::Python(_) => "python",
            ProviderConfig::Node(_) => "node",
            ProviderConfig::NodeStatic(_) => "node-static",
            ProviderConfig::Php(_) => "php",
            ProviderConfig::Wordpress(_) => "wordpress",
            ProviderConfig::Laravel(_) => "laravel",
            ProviderConfig::Go(_) => "go",
            ProviderConfig::StaticFile(_) => "staticfile",
            ProviderConfig::Hugo(_) => "hugo",
            ProviderConfig::Jekyll(_) => "jekyll",
            ProviderConfig::Mkdocs(_) => "mkdocs",
        }
    }

    /// `model_dump(mode="json")` equivalent: the JSON view consumed by the
    /// evaluation host and the wasmer annotations.
    pub fn to_json(&self) -> serde_json::Value {
        each_config!(self, config => {
            serde_json::to_value(config).expect("config serializes")
        })
    }

    pub fn base(&self) -> &BaseConfig {
        each_config!(self, config => config.base())
    }

    pub fn base_mut(&mut self) -> &mut BaseConfig {
        each_config!(self, config => config.base_mut())
    }

    /// Set `cross_platform` when the provider supports it (python only).
    pub fn set_cross_platform(&mut self, value: &str) -> bool {
        match self {
            ProviderConfig::Python(config) => {
                config.cross_platform = Some(value.to_owned());
                true
            }
            ProviderConfig::Mkdocs(config) => config.set_cross_platform(value),
            _ => false,
        }
    }
}

/// Port of `generator.detect_provider`: highest score wins, ties broken
/// by registry order.
pub fn detect_provider(path: &Path, base: &BaseConfig) -> Result<&'static str> {
    let mut matches: Vec<(usize, &'static str, DetectResult)> = Vec::new();
    for (index, (name, detect)) in REGISTRY.iter().enumerate() {
        if let Some(result) = detect(path, base) {
            matches.push((index, name, result));
        }
    }
    matches.sort_by(|a, b| b.2.score.cmp(&a.2.score).then(a.0.cmp(&b.0)));
    matches
        .first()
        .map(|(_, name, _)| *name)
        .ok_or_else(|| anyhow!("Shipit could not detect a provider for this project"))
}

/// Port of `generator.load_provider`.
pub fn load_provider(
    path: &Path,
    base: &BaseConfig,
    use_provider: Option<&str>,
) -> Result<&'static str> {
    if let Some(wanted) = use_provider {
        let wanted = wanted.to_lowercase();
        if let Some((name, _)) = REGISTRY
            .iter()
            .find(|(name, _)| name.to_lowercase() == wanted)
        {
            return Ok(name);
        }
    }
    detect_provider(path, base)
}

/// The declared field defaults for a provider config (pydantic's notion
/// for `exclude_defaults`). Call inside `base::without_env` for the pure
/// declared defaults.
pub fn defaults_json(name: &str) -> Result<serde_json::Value> {
    let value = match name {
        "python" => serde_json::to_value(python::PythonConfig::default())?,
        "node" => serde_json::to_value(node::NodeConfig::default())?,
        "node-static" => serde_json::to_value(node_static::NodeStaticConfig::default())?,
        "php" => serde_json::to_value(php::PhpConfig::default())?,
        "wordpress" => serde_json::to_value(wordpress::WordPressConfig::default())?,
        "laravel" => serde_json::to_value(laravel::LaravelConfig::default())?,
        "go" => serde_json::to_value(go::GoConfig::default())?,
        "staticfile" => serde_json::to_value(staticfile::StaticFileConfig::default())?,
        "hugo" => serde_json::to_value(hugo::HugoConfig::default())?,
        "jekyll" => serde_json::to_value(jekyll::JekyllConfig::default())?,
        "mkdocs" => serde_json::to_value(mkdocs::MkdocsConfig::default())?,
        other => return Err(anyhow!("unknown provider {other:?}")),
    };
    Ok(value)
}

/// Deserialize a config JSON back into the provider's typed config.
pub fn config_from_json(name: &str, json: serde_json::Value) -> Result<ProviderConfig> {
    Ok(match name {
        "python" => ProviderConfig::Python(serde_json::from_value(json)?),
        "node" => ProviderConfig::Node(serde_json::from_value(json)?),
        "node-static" => ProviderConfig::NodeStatic(serde_json::from_value(json)?),
        "php" => ProviderConfig::Php(serde_json::from_value(json)?),
        "wordpress" => ProviderConfig::Wordpress(serde_json::from_value(json)?),
        "laravel" => ProviderConfig::Laravel(serde_json::from_value(json)?),
        "go" => ProviderConfig::Go(serde_json::from_value(json)?),
        "staticfile" => ProviderConfig::StaticFile(serde_json::from_value(json)?),
        "hugo" => ProviderConfig::Hugo(serde_json::from_value(json)?),
        "jekyll" => ProviderConfig::Jekyll(serde_json::from_value(json)?),
        "mkdocs" => ProviderConfig::Mkdocs(serde_json::from_value(json)?),
        other => return Err(anyhow!("unknown provider {other:?}")),
    })
}

/// Port of the `--config` JSON merge in `generator.load_provider_config`:
/// `model_dump() | patch`, revalidated into the typed config.
pub fn merge_config_json(
    name: &str,
    config: &ProviderConfig,
    patch: &serde_json::Value,
) -> Result<ProviderConfig> {
    let mut merged = config.to_json();
    let (Some(target), Some(patch_map)) = (merged.as_object_mut(), patch.as_object()) else {
        return Err(anyhow!("Config must be a dictionary"));
    };
    for (key, value) in patch_map {
        target.insert(key.clone(), value.clone());
    }
    config_from_json(name, merged)
}

/// Port of `generator.load_provider_config` (without the --config JSON
/// merge, which lives with the CLI).
pub fn load_provider_config(name: &str, path: &Path, base: BaseConfig) -> Result<ProviderConfig> {
    let mut config = match name {
        "python" => ProviderConfig::Python(python::load_config(path, base)),
        "node" => ProviderConfig::Node(node::load_config(path, base)),
        "node-static" => ProviderConfig::NodeStatic(node_static::load_config(path, base)?),
        "php" => ProviderConfig::Php(php::load_config(path, base)),
        "wordpress" => ProviderConfig::Wordpress(wordpress::load_config(path, base)),
        "laravel" => ProviderConfig::Laravel(laravel::load_config(path, base)),
        "go" => ProviderConfig::Go(go::load_config(path, base)?),
        "staticfile" => ProviderConfig::StaticFile(staticfile::load_config(path, base)?),
        "hugo" => ProviderConfig::Hugo(hugo::load_config(path, base)?),
        "jekyll" => ProviderConfig::Jekyll(jekyll::load_config(path, base)?),
        "mkdocs" => ProviderConfig::Mkdocs(mkdocs::load_config(path, base)?),
        other => return Err(anyhow!("unknown provider {other:?}")),
    };
    if config.base().name.is_none() {
        let dir_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        config.base_mut().name = Some(dir_name);
    }
    Ok(config)
}

#[cfg(test)]
mod config_inheritance_tests {
    use super::*;

    const BASE_FIELDS: &[&str] = &["name", "port", "commands", "app_subdir"];
    const NODE_FIELDS: &[&str] = &[
        "use_edgejs",
        "precompile_edgejs",
        "package_manager",
        "framework",
        "extra_dependencies",
        "build_command",
        "node_version",
        "npm_version",
        "pnpm_version",
        "yarn_version",
        "bun_version",
        "optimize_node_dependencies",
        "remove_native_binaries",
        "install_requires_all_files",
        "install_inputs",
        "package_name",
    ];

    fn keys(value: &serde_json::Value) -> Vec<&str> {
        value
            .as_object()
            .expect("config object")
            .keys()
            .map(String::as_str)
            .collect()
    }

    #[test]
    fn node_config_fields_are_flattened_in_declaration_order() {
        let node = defaults_json("node").unwrap();
        let node_static = defaults_json("node-static").unwrap();
        let laravel = defaults_json("laravel").unwrap();

        let expected_node: Vec<&str> = BASE_FIELDS
            .iter()
            .copied()
            .chain(NODE_FIELDS.iter().copied())
            .collect();
        let expected_node_static: Vec<&str> = BASE_FIELDS
            .iter()
            .copied()
            .chain([
                "convert_redirects",
                "sws_version",
                "static_dir",
                "redirects_config",
            ])
            .chain(NODE_FIELDS.iter().copied())
            .collect();
        let expected_laravel: Vec<&str> = BASE_FIELDS
            .iter()
            .copied()
            .chain(NODE_FIELDS.iter().copied())
            .chain([
                "phpix",
                "use_composer",
                "composer_build_script",
                "php_version",
                "php_architecture",
                "phpix_worker_threads",
                "public_dir",
            ])
            .collect();

        assert_eq!(keys(&node), expected_node);
        assert_eq!(keys(&node_static), expected_node_static);
        assert_eq!(keys(&laravel), expected_laravel);
    }

    #[test]
    fn flattened_node_configs_round_trip_without_nested_fields() {
        for name in ["node", "node-static", "laravel"] {
            let json = defaults_json(name).unwrap();
            assert!(!json.as_object().unwrap().contains_key("node"));
            let round_trip = config_from_json(name, json.clone()).unwrap().to_json();
            assert_eq!(keys(&round_trip), keys(&json));
            assert_eq!(round_trip, json);
        }
    }
}
