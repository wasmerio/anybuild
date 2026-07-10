//! Port of `src/shipit/providers/laravel.py`.
//!
//! `LaravelConfig(PhpConfig, NodeConfig)`: the config is the php config
//! (with `use_composer` forced on) merged with the node config (minus its
//! `framework`) and the base config, exactly as Python's
//! `config.model_dump() | node_config_data | base_config.model_dump()`.
//! The merge happens on the JSON views so this port stays decoupled from
//! the node provider's struct layout.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::base::{BaseConfig, DetectResult, HasBase};
use crate::php;

pub const NAME: &str = "laravel";

fn default_false() -> Option<bool> {
    Some(false)
}

fn default_true() -> Option<bool> {
    Some(true)
}

fn default_node_version() -> Option<String> {
    Some("24".to_owned())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaravelConfig {
    #[serde(flatten)]
    pub base: BaseConfig,
    // Node-side fields (mirroring NodeConfig; kept loosely typed so this
    // module compiles independently of crate::node's port). The serde
    // defaults are the declared pydantic defaults.
    #[serde(default = "default_false")]
    pub use_edgejs: Option<bool>,
    #[serde(default)]
    pub precompile_edgejs: Option<bool>,
    #[serde(default)]
    pub package_manager: Option<String>,
    /// `Optional[Any]` in Python; holds the php framework value.
    #[serde(default)]
    pub framework: Option<Value>,
    #[serde(default)]
    pub extra_dependencies: BTreeSet<String>,
    #[serde(default)]
    pub build_command: Option<String>,
    #[serde(default = "default_node_version")]
    pub node_version: Option<String>,
    #[serde(default)]
    pub npm_version: Option<String>,
    #[serde(default)]
    pub pnpm_version: Option<String>,
    #[serde(default)]
    pub yarn_version: Option<String>,
    #[serde(default)]
    pub bun_version: Option<String>,
    #[serde(default = "default_true")]
    pub optimize_node_dependencies: Option<bool>,
    #[serde(default = "default_false")]
    pub remove_native_binaries: Option<bool>,
    #[serde(default)]
    pub install_requires_all_files: bool,
    #[serde(default)]
    pub install_inputs: Option<Vec<String>>,
    #[serde(default)]
    pub package_name: Option<String>,
    // Php-side fields (minus framework, which node's position holds).
    pub phpix: bool,
    pub use_composer: bool,
    pub composer_build_script: Option<String>,
    pub php_version: Option<String>,
    pub php_architecture: Option<String>,
    pub phpix_worker_threads: Option<i64>,
    pub public_dir: Option<String>,
}

impl Default for LaravelConfig {
    fn default() -> Self {
        Self {
            base: BaseConfig::default(),
            use_edgejs: Some(false),
            precompile_edgejs: None,
            package_manager: None,
            framework: None,
            extra_dependencies: BTreeSet::new(),
            build_command: None,
            node_version: Some("24".to_owned()),
            npm_version: None,
            pnpm_version: None,
            yarn_version: None,
            bun_version: None,
            optimize_node_dependencies: Some(true),
            remove_native_binaries: Some(false),
            install_requires_all_files: false,
            install_inputs: None,
            package_name: None,
            phpix: false,
            use_composer: false,
            composer_build_script: None,
            php_version: Some("8.3.29".to_owned()),
            php_architecture: None,
            phpix_worker_threads: Some(4),
            public_dir: None,
        }
    }
}

impl HasBase for LaravelConfig {
    fn base(&self) -> &BaseConfig {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseConfig {
        &mut self.base
    }
}

fn to_object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        other => panic!("config serializes to an object, got {other}"),
    }
}

/// Port of `LaravelProvider.load_config`.
pub fn load_config(path: &Path, base: BaseConfig) -> LaravelConfig {
    let mut php_config = php::load_config(path, base.clone());
    php_config.use_composer = true;
    // Python passes infer_start=False, but the inferred commands are
    // overwritten by the base config in the final merge anyway.
    let node_config = crate::node::load_config(path, base.clone());

    let mut merged = to_object(serde_json::to_value(&php_config).expect("php config serializes"));
    let mut node_data =
        to_object(serde_json::to_value(&node_config).expect("node config serializes"));
    node_data.remove("framework");
    merged.extend(node_data);
    merged.extend(to_object(
        serde_json::to_value(&base).expect("base config serializes"),
    ));

    serde_json::from_value(Value::Object(merged)).expect("laravel config deserializes")
}

/// Port of `LaravelProvider.detect`.
pub fn detect(path: &Path, _base: &BaseConfig) -> Option<DetectResult> {
    if path.join("artisan").exists() && path.join("composer.json").exists() {
        return Some(DetectResult { name: NAME, score: 95 });
    }
    None
}
