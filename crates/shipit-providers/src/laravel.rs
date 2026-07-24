//! Port of `src/shipit/providers/laravel.py`.
//!
//! `LaravelConfig(PhpConfig, NodeConfig)`: the config is the php config
//! (with `use_composer` forced on) merged with the node config (minus its
//! `framework`) and the base config, exactly as Python's
//! `config.model_dump() | node_config_data | base_config.model_dump()`.
//! The shared Node fields are flattened from `NodeConfigFields`; loading
//! still uses the Python-compatible JSON merge to preserve precedence.

use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::base::{BaseConfig, DetectResult, HasBase};
use crate::node::NodeConfigFields;
use crate::php;

pub const NAME: &str = "laravel";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaravelConfig {
    #[serde(flatten)]
    pub base: BaseConfig,
    #[serde(flatten)]
    pub node: NodeConfigFields<Value>,
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
            node: NodeConfigFields::default(),
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

impl std::ops::Deref for LaravelConfig {
    type Target = NodeConfigFields<Value>;

    fn deref(&self) -> &Self::Target {
        &self.node
    }
}

impl std::ops::DerefMut for LaravelConfig {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.node
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
pub fn load_config(path: &Path, base: BaseConfig) -> Result<LaravelConfig> {
    let mut php_config = php::load_config(path, base.clone())?;
    php_config.use_composer = true;
    // Python passes infer_start=False, but the inferred commands are
    // overwritten by the base config in the final merge anyway.
    let node_config = crate::node::load_config(path, base.clone())?;

    let mut merged = to_object(serde_json::to_value(&php_config).expect("php config serializes"));
    let mut node_data =
        to_object(serde_json::to_value(&node_config).expect("node config serializes"));
    node_data.remove("framework");
    merged.extend(node_data);
    merged.extend(to_object(
        serde_json::to_value(&base).expect("base config serializes"),
    ));

    Ok(serde_json::from_value(Value::Object(merged)).expect("laravel config deserializes"))
}

/// Port of `LaravelProvider.detect`.
pub fn detect(path: &Path, _base: &BaseConfig) -> Option<DetectResult> {
    if path.join("artisan").exists() && path.join("composer.json").exists() {
        return Some(DetectResult {
            name: NAME,
            score: 95,
        });
    }
    None
}
