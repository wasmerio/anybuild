//! Laravel provider implementation.
//!
//! Laravel combines PHP configuration with the Node build toolchain fields
//! used to compile frontend assets. Node application/runtime fields are
//! intentionally excluded because PHP serves the resulting application.

use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::operation::OperationContext;
use crate::providers::base::{BaseConfig, HasBase};
use crate::providers::node::NodeBuildConfigFields;
use crate::providers::php::{self, PhpFramework};
use crate::providers::Provider;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaravelConfig {
    #[serde(flatten)]
    pub base: BaseConfig,
    #[serde(flatten)]
    pub node: NodeBuildConfigFields,
    pub php_framework: Option<PhpFramework>,
    /// Unspecified lets the selected runner choose its preferred engine.
    pub phpix: Option<bool>,
    #[serde(rename = "composer_enable")]
    pub use_composer: bool,
    #[serde(rename = "composer_build_script")]
    pub composer_build_script: Option<String>,
    pub php_version: Option<String>,
    pub php_architecture: Option<String>,
    pub phpix_worker_threads: Option<i64>,
    #[serde(rename = "php_public_dir")]
    pub public_dir: Option<String>,
}

impl Default for LaravelConfig {
    fn default() -> Self {
        Self {
            base: BaseConfig::default(),
            node: NodeBuildConfigFields::default(),
            php_framework: None,
            phpix: None,
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
    type Target = NodeBuildConfigFields;

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
pub fn load_config(
    path: &Path,
    base: BaseConfig,
    operation: &OperationContext,
) -> Result<LaravelConfig> {
    let mut php_config = php::load_config(path, base.clone(), operation)?;
    php_config.use_composer = true;
    let node_config = crate::providers::node::load_build_config(path, &base, operation)?;

    let mut merged = to_object(serde_json::to_value(&php_config).expect("php config serializes"));
    let node_data =
        to_object(serde_json::to_value(&node_config).expect("node build config serializes"));
    merged.extend(node_data);
    merged.extend(to_object(
        serde_json::to_value(&base).expect("base config serializes"),
    ));

    Ok(serde_json::from_value(Value::Object(merged)).expect("laravel config deserializes"))
}

impl Provider for LaravelConfig {
    type Evidence = ();

    const NAME: &'static str = "laravel";
    const DETECTION_DETAILS: &'static [(&'static str, &'static str)] = &[
        ("Package manager", "node_package_manager"),
        ("PHP version", "php_version"),
    ];

    fn detection_evidence(
        path: &Path,
        _base: &BaseConfig,
        _operation: &OperationContext,
    ) -> Option<Self::Evidence> {
        (path.join("artisan").exists() && path.join("composer.json").exists()).then_some(())
    }

    fn load(path: &Path, base: BaseConfig, operation: &OperationContext) -> Result<Self> {
        load_config(path, base, operation)
    }
}
