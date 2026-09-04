//! Project detection and provider configuration.
//!
//! Provider registry and generation dispatch.
//! Each typed provider config implements `Provider`; selection scores
//! live here and registry order breaks ties.

use std::path::Path;

use anyhow::{anyhow, Context, Result};
use serde::{de::DeserializeOwned, Serialize};

use crate::event::ProviderDetail;
use crate::operation::OperationContext;

pub mod base;
mod environment;
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

pub use base::{BaseConfig, HasBase};
pub(crate) use environment::apply_environment;

const SNAPSHOT_EXCLUDED_FIELDS: &[&str] = &[
    "name",
    "port",
    "app_subdir",
    "node_install_inputs",
    "python_install_inputs",
    "static_redirects_config",
    "node_package_name",
    // Compatibility-only alias; new files persist the common service.
    "python_database",
];

pub(crate) trait Provider: HasBase + Serialize + DeserializeOwned + Default + Sized {
    type Evidence: Copy;

    const NAME: &'static str;
    const CONFIG_SCHEMA: u32 = 1;
    const DETECTION_DETAILS: &'static [(&'static str, &'static str)] = &[];

    fn detection_evidence(
        path: &Path,
        base: &BaseConfig,
        operation: &OperationContext,
    ) -> Option<Self::Evidence>;

    fn load(path: &Path, base: BaseConfig, operation: &OperationContext) -> Result<Self>;

    fn copy_transient_fields_from(&mut self, source: &Self) {
        self.base_mut().runtime_dependencies = source.base().runtime_dependencies.clone();
    }

    fn runtime_dependencies(&self) -> Vec<String> {
        self.base().runtime_dependencies.clone()
    }

    fn detect(
        path: &Path,
        base: &BaseConfig,
        operation: &OperationContext,
    ) -> Result<Option<Self>> {
        Self::detection_evidence(path, base, operation)
            .map(|_| Self::load(path, base.clone(), operation))
            .transpose()
    }

    fn detection_details(&self) -> Vec<ProviderDetail> {
        detection_details_from_fields(self, Self::DETECTION_DETAILS, Self::format_detection_detail)
    }

    fn format_detection_detail(_field: &str, value: &str) -> String {
        value.to_owned()
    }

    fn provider_name(&self) -> &'static str {
        Self::NAME
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("config serializes")
    }

    fn defaults_json() -> Result<serde_json::Value> {
        Ok(serde_json::to_value(Self::default())?)
    }

    fn exclude_defaults_json(&self) -> serde_json::Value {
        let defaults = Self::defaults_json().expect("default config serializes");
        exclude_defaults(self.to_json(), defaults)
    }

    fn persisted_json(&self) -> serde_json::Value {
        persisted_config_json(self)
    }

    fn from_json(json: serde_json::Value) -> Result<Self> {
        Ok(serde_json::from_value(json)?)
    }

    fn merge_json(&self, patch: &serde_json::Value) -> Result<Self> {
        let mut merged = self.to_json();
        if !merged.is_object() || !patch.is_object() {
            return Err(anyhow!("Config must be a dictionary"));
        }
        merge_json_value(&mut merged, patch);
        let mut config = Self::from_json(merged)?;
        config.copy_transient_fields_from(self);
        Ok(config)
    }

    fn apply_workspace_config(&mut self, _workspace_root: &Path) {}

    fn validate(&self, _path: &Path) -> Result<()> {
        Ok(())
    }
}

fn merge_json_value(target: &mut serde_json::Value, patch: &serde_json::Value) {
    match (target, patch) {
        (serde_json::Value::Object(target), serde_json::Value::Object(patch)) => {
            for (key, value) in patch {
                match target.get_mut(key) {
                    Some(target) if target.is_object() && value.is_object() => {
                        merge_json_value(target, value);
                    }
                    _ => {
                        target.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (target, patch) => *target = patch.clone(),
    }
}

macro_rules! provider_registry {
    ($($variant:ident => $config:path),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) enum ProviderKind {
            $($variant),+
        }

        /// Order resolves equal scores and remains compatible with the
        /// original provider registry.
        const REGISTRY: &[ProviderKind] = &[$(ProviderKind::$variant),+];

        /// Every provider's loaded config.
        #[derive(Debug, Clone)]
        pub enum ProviderConfig {
            $($variant($config)),+
        }

        impl ProviderConfig {
            pub fn provider_name(&self) -> &'static str {
                match self {
                    $(Self::$variant(config) => config.provider_name()),+
                }
            }

            /// The JSON view consumed by the evaluation host and annotations.
            pub fn to_json(&self) -> serde_json::Value {
                match self {
                    $(Self::$variant(config) => config.to_json()),+
                }
            }

            pub fn base(&self) -> &BaseConfig {
                match self {
                    $(Self::$variant(config) => config.base()),+
                }
            }

            pub fn base_mut(&mut self) -> &mut BaseConfig {
                match self {
                    $(Self::$variant(config) => config.base_mut()),+
                }
            }

            pub(crate) fn detection_details(&self) -> Vec<ProviderDetail> {
                match self {
                    $(Self::$variant(config) => config.detection_details()),+
                }
            }

            pub(crate) fn apply_workspace_config(&mut self, workspace_root: &Path) {
                match self {
                    $(Self::$variant(config) => config.apply_workspace_config(workspace_root)),+
                }
            }

            pub(crate) fn validate(&self, path: &Path) -> Result<()> {
                match self {
                    $(Self::$variant(config) => config.validate(path)),+
                }
            }

            pub(crate) fn merge_json(&self, patch: &serde_json::Value) -> Result<Self> {
                match self {
                    $(Self::$variant(config) => config.merge_json(patch).map(Self::$variant)),+
                }
            }

            fn exclude_defaults_json(&self) -> serde_json::Value {
                match self {
                    $(Self::$variant(config) => config.exclude_defaults_json()),+
                }
            }

            pub(crate) fn persisted_json(&self) -> serde_json::Value {
                match self {
                    $(Self::$variant(config) => config.persisted_json()),+
                }
            }

            pub(crate) fn runtime_dependencies(&self) -> Vec<String> {
                match self {
                    $(Self::$variant(config) => config.runtime_dependencies()),+
                }
            }

            pub(crate) fn copy_transient_fields_from(&mut self, source: &Self) {
                match (self, source) {
                    $((Self::$variant(target), Self::$variant(source)) => {
                        target.copy_transient_fields_from(source);
                    },)+
                    _ => {}
                }
            }

            pub(crate) fn config_schema(&self) -> u32 {
                match self {
                    $(Self::$variant(_) => <$config as Provider>::CONFIG_SCHEMA),+
                }
            }
        }

        impl ProviderKind {
            pub(crate) fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => <$config as Provider>::NAME),+
                }
            }

            pub(crate) fn from_name(name: &str) -> Option<Self> {
                REGISTRY
                    .iter()
                    .copied()
                    .find(|kind| kind.name().eq_ignore_ascii_case(name))
            }

            fn detect(
                self,
                path: &Path,
                base: &BaseConfig,
                operation: &OperationContext,
            ) -> Result<Option<ProviderConfig>> {
                match self {
                    $(Self::$variant => detect_typed(path, base, operation, ProviderConfig::$variant)),+
                }
            }

            pub(crate) fn load(
                self,
                path: &Path,
                base: &BaseConfig,
                operation: &OperationContext,
            ) -> Result<ProviderConfig> {
                match self {
                    $(Self::$variant => load_typed(path, base, operation, ProviderConfig::$variant)),+
                }
            }

            fn defaults_json(self) -> Result<serde_json::Value> {
                match self {
                    $(Self::$variant => <$config as Provider>::defaults_json()),+
                }
            }

            fn config_schema(self) -> u32 {
                match self {
                    $(Self::$variant => <$config as Provider>::CONFIG_SCHEMA),+
                }
            }

            fn config_from_json(self, json: serde_json::Value) -> Result<ProviderConfig> {
                match self {
                    $(Self::$variant => from_json_typed(json, ProviderConfig::$variant)),+
                }
            }
        }
    };
}

provider_registry! {
    Laravel => laravel::LaravelConfig,
    Hugo => hugo::HugoConfig,
    Mkdocs => mkdocs::MkdocsConfig,
    Python => python::PythonConfig,
    Wordpress => wordpress::WordPressConfig,
    Php => php::PhpConfig,
    NodeStatic => node_static::NodeStaticConfig,
    Node => node::NodeConfig,
    Jekyll => jekyll::JekyllConfig,
    Go => go::GoConfig,
    StaticFile => staticfile::StaticFileConfig,
}

impl ProviderConfig {
    /// Set `cross_platform` when the provider supports it (python only).
    #[cfg(test)]
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

pub(crate) fn detection_details_from_fields<T: Serialize>(
    config: &T,
    fields: &[(&str, &str)],
    format: fn(&str, &str) -> String,
) -> Vec<ProviderDetail> {
    let config = serde_json::to_value(config).expect("config serializes");
    fields
        .iter()
        .filter_map(|(label, field)| {
            let value = config
                .get(field)?
                .as_str()
                .filter(|value| !value.is_empty())?;
            Some(ProviderDetail {
                label: (*label).to_owned(),
                value: format(field, value),
            })
        })
        .collect()
}

pub(crate) fn humanize(value: &str) -> String {
    value
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(capitalize)
        .collect::<Vec<_>>()
        .join(" ")
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

fn detect_typed<C: Provider>(
    path: &Path,
    base: &BaseConfig,
    operation: &OperationContext,
    wrap: fn(C) -> ProviderConfig,
) -> Result<Option<ProviderConfig>> {
    C::detect(path, base, operation).map(|config| config.map(wrap))
}

fn load_typed<C: Provider>(
    path: &Path,
    base: &BaseConfig,
    operation: &OperationContext,
    wrap: fn(C) -> ProviderConfig,
) -> Result<ProviderConfig> {
    C::load(path, base.clone(), operation).map(wrap)
}

fn from_json_typed<C: Provider>(
    json: serde_json::Value,
    wrap: fn(C) -> ProviderConfig,
) -> Result<ProviderConfig> {
    C::from_json(json).map(wrap)
}

impl ProviderKind {
    fn detection_score(
        self,
        path: &Path,
        base: &BaseConfig,
        operation: &OperationContext,
    ) -> Option<i32> {
        Some(match self {
            Self::Laravel => {
                laravel::LaravelConfig::detection_evidence(path, base, operation)?;
                95
            }
            Self::Hugo => match hugo::HugoConfig::detection_evidence(path, base, operation)? {
                hugo::DetectionEvidence::Strong => 80,
                hugo::DetectionEvidence::Structural => 40,
            },
            Self::Mkdocs => {
                mkdocs::MkdocsConfig::detection_evidence(path, base, operation)?;
                85
            }
            Self::Python => {
                match python::PythonConfig::detection_evidence(path, base, operation)? {
                    python::DetectionEvidence::DjangoDependencies => 70,
                    python::DetectionEvidence::Dependencies => 50,
                    python::DetectionEvidence::Command => 80,
                    python::DetectionEvidence::Entrypoint => 10,
                }
            }
            Self::Wordpress => {
                match wordpress::WordPressConfig::detection_evidence(path, base, operation)? {
                    wordpress::DetectionEvidence::Site => 80,
                    wordpress::DetectionEvidence::Extension => 75,
                }
            }
            Self::Php => match php::PhpConfig::detection_evidence(path, base, operation)? {
                php::DetectionEvidence::DrupalWeb => 70,
                php::DetectionEvidence::Framework => 65,
                php::DetectionEvidence::ComposerEntrypoint => 60,
                php::DetectionEvidence::Entrypoint => 20,
                php::DetectionEvidence::PhpFile => 20,
                php::DetectionEvidence::ComposerProject => 20,
                php::DetectionEvidence::StartCommand => 70,
                php::DetectionEvidence::InstallCommand => 30,
            },
            Self::NodeStatic => {
                match node_static::NodeStaticConfig::detection_evidence(path, base, operation)? {
                    node_static::DetectionEvidence::Strong => 60,
                    node_static::DetectionEvidence::Weak => 20,
                }
            }
            Self::Node => match node::NodeConfig::detection_evidence(path, base, operation)? {
                node::DetectionEvidence::StartCommand => 35,
                node::DetectionEvidence::PackageWithStart => 30,
                node::DetectionEvidence::Package => 10,
                node::DetectionEvidence::FrameworkWithStart => 45,
                node::DetectionEvidence::Framework => 10,
                node::DetectionEvidence::Entrypoint => 30,
            },
            Self::Jekyll => {
                match jekyll::JekyllConfig::detection_evidence(path, base, operation)? {
                    jekyll::DetectionEvidence::Strong => 85,
                    jekyll::DetectionEvidence::Structural => 40,
                }
            }
            Self::Go => {
                go::GoConfig::detection_evidence(path, base, operation)?;
                80
            }
            Self::StaticFile => {
                match staticfile::StaticFileConfig::detection_evidence(path, base, operation)? {
                    staticfile::DetectionEvidence::Staticfile => 50,
                    staticfile::DetectionEvidence::Html
                    | staticfile::DetectionEvidence::UnbuiltNodeSite => 15,
                    staticfile::DetectionEvidence::Fallback => 10,
                    staticfile::DetectionEvidence::StartCommand => 70,
                }
            }
        })
    }
}

struct Candidate {
    kind: ProviderKind,
    registry_index: usize,
    score: i32,
    result: Result<ProviderConfig>,
    events: crate::operation::CapturedEvents,
}

pub(crate) fn select_provider(
    path: &Path,
    base: &BaseConfig,
    use_provider: Option<&str>,
    operation: &OperationContext,
) -> Result<(ProviderKind, ProviderConfig)> {
    if let Some(kind) = use_provider.and_then(ProviderKind::from_name) {
        let config = finish_config(path, kind.load(path, base, operation)?);
        return Ok((kind, config));
    }

    let mut candidates = Vec::new();
    for (registry_index, kind) in REGISTRY.iter().copied().enumerate() {
        let (candidate_operation, events) = operation.capture_events();
        let Some(result) = kind.detect(path, base, &candidate_operation).transpose() else {
            continue;
        };
        candidates.push(Candidate {
            kind,
            registry_index,
            score: 0,
            result,
            events,
        });
    }
    for candidate in &mut candidates {
        candidate.score = candidate
            .kind
            .detection_score(path, base, operation)
            .ok_or_else(|| {
                anyhow!(
                    "{} returned a config without matching its detection evidence",
                    candidate.kind.name()
                )
            })?;
    }
    candidates.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(a.registry_index.cmp(&b.registry_index))
    });
    let selected = candidates
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("Anybuild could not detect a provider for this project"))?;
    selected.events.replay_into(operation);
    let config = finish_config(path, selected.result?);
    Ok((selected.kind, config))
}

fn finish_config(path: &Path, mut config: ProviderConfig) -> ProviderConfig {
    if base::is_blank(&config.base().name) {
        config.base_mut().name = Some(
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
    }
    config
}

pub(crate) fn finalize_config(path: &Path, mut config: ProviderConfig) -> ProviderConfig {
    if let ProviderConfig::Python(python) = &mut config {
        if let Some(database) = python.database {
            python
                .base
                .set_database_service(database.into_database_engine());
        }
    }
    finish_config(path, config)
}

/// The declared field defaults for a provider config (pydantic's notion
/// for `exclude_defaults`). Defaults never apply the environment overlay.
pub fn defaults_json(name: &str) -> Result<serde_json::Value> {
    ProviderKind::from_name(name)
        .ok_or_else(|| anyhow!("unknown provider {name:?}"))?
        .defaults_json()
}

pub(crate) fn provider_schema(name: &str) -> Result<u32> {
    let kind = ProviderKind::from_name(name).ok_or_else(|| anyhow!("unknown provider {name:?}"))?;
    Ok(kind.config_schema())
}

/// Pydantic's `model_dump(mode="json", exclude_defaults=True)` for a typed
/// provider config.
pub fn exclude_defaults_json(config: &ProviderConfig) -> serde_json::Value {
    config.exclude_defaults_json()
}

/// The stable, provider-owned portion written into an Anybuild file.
///
/// Values that merely cache project analysis remain live, while runtime
/// versions are pinned even when they currently equal the provider default.
fn persisted_config_json<C: Provider>(config: &C) -> serde_json::Value {
    let full = config.to_json();
    let reduced = config.exclude_defaults_json();
    let Some(full) = full.as_object() else {
        return serde_json::json!({});
    };
    let reduced = reduced.as_object();
    let mut persisted = serde_json::Map::new();
    for (key, value) in full {
        if SNAPSHOT_EXCLUDED_FIELDS.contains(&key.as_str()) {
            continue;
        }
        let pin = key.ends_with("_version");
        let changed = reduced.and_then(|values| values.get(key));
        if pin && !value.is_null() {
            persisted.insert(key.clone(), value.clone());
        } else if let Some(changed) = changed {
            persisted.insert(key.clone(), changed.clone());
        }
    }
    serde_json::Value::Object(persisted)
}

pub(crate) fn load_explicit_provider(
    name: &str,
    path: &Path,
    base: &BaseConfig,
    operation: &OperationContext,
) -> Result<ProviderConfig> {
    let kind = ProviderKind::from_name(name).ok_or_else(|| anyhow!("unknown provider {name:?}"))?;
    kind.load(path, base, &operation.without_environment())
        .map(|config| finish_config(path, config))
        .with_context(|| format!("loading {name} config"))
}

/// Apply a provider's declared defaults to an already serialized config.
/// This is used by the Wasmer runner after applying runtime overrides.
pub fn exclude_defaults_from_json(name: &str, dumped: serde_json::Value) -> serde_json::Value {
    let Ok(defaults) = defaults_json(name) else {
        return dumped;
    };
    exclude_defaults(dumped, defaults)
}

fn exclude_defaults(dumped: serde_json::Value, defaults: serde_json::Value) -> serde_json::Value {
    let serde_json::Value::Object(defaults) = defaults else {
        return dumped;
    };
    match dumped {
        serde_json::Value::Object(dumped) => exclude_default_object(dumped, &defaults),
        dumped => dumped,
    }
}

fn exclude_default_object(
    dumped: serde_json::Map<String, serde_json::Value>,
    defaults: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    for (key, value) in dumped {
        if value.as_array().is_some_and(Vec::is_empty) {
            continue;
        }
        match defaults.get(&key) {
            Some(default) if *default == value => {}
            Some(serde_json::Value::Object(default_child)) => {
                if let serde_json::Value::Object(child) = value {
                    let reduced = exclude_default_object(child, default_child);
                    if reduced.as_object().is_none_or(|map| !map.is_empty()) {
                        out.insert(key, reduced);
                    }
                } else {
                    out.insert(key, value);
                }
            }
            _ => {
                out.insert(key, value);
            }
        }
    }
    serde_json::Value::Object(out)
}

/// Deserialize a config JSON back into the provider's typed config.
pub fn config_from_json(name: &str, json: serde_json::Value) -> Result<ProviderConfig> {
    ProviderKind::from_name(name)
        .ok_or_else(|| anyhow!("unknown provider {name:?}"))?
        .config_from_json(json)
}

#[cfg(test)]
pub(crate) fn load_provider_for_test(
    path: &Path,
    base: &BaseConfig,
    use_provider: Option<&str>,
) -> Result<&'static str> {
    select_provider(path, base, use_provider, &OperationContext::for_test())
        .map(|(kind, _)| kind.name())
}

#[cfg(test)]
pub(crate) fn load_provider_config_for_test(
    name: &str,
    path: &Path,
    base: BaseConfig,
) -> Result<ProviderConfig> {
    let kind = ProviderKind::from_name(name).ok_or_else(|| anyhow!("unknown provider {name:?}"))?;
    kind.load(path, &base, &OperationContext::for_test())
        .map(|config| finish_config(path, config))
}

#[cfg(test)]
pub(crate) fn detection_score_for_test(name: &str, path: &Path, base: &BaseConfig) -> Option<i32> {
    ProviderKind::from_name(name)?.detection_score(path, base, &OperationContext::for_test())
}

#[cfg(test)]
mod selection_tests {
    use std::sync::{Arc, Mutex};

    use indexmap::IndexMap;

    use super::*;
    use crate::event::{DiagnosticLevel, Event, ProcessIo, Reporter};

    fn operation(
        environment: IndexMap<String, String>,
    ) -> (OperationContext, Arc<Mutex<Vec<Event>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        let operation = OperationContext::new(
            environment,
            false,
            ProcessIo::Inherit,
            Reporter::new(move |event: &Event| {
                captured.lock().unwrap().push(event.clone());
            }),
        );
        (operation, events)
    }

    #[test]
    fn losing_candidate_diagnostics_are_discarded() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(
            project.path().join("package.json"),
            r#"{"scripts":{"build":"vite build"},"dependencies":{"vite":"7.0.0"}}"#,
        )
        .unwrap();
        let (operation, events) = operation(IndexMap::new());

        let (kind, _) =
            select_provider(project.path(), &BaseConfig::default(), None, &operation).unwrap();

        assert_eq!(kind, ProviderKind::NodeStatic);
        assert!(
            !events
                .lock()
                .unwrap()
                .iter()
                .any(|event| matches!(event, Event::Diagnostic { .. })),
            "the losing Node provider must not emit missing-start warnings"
        );
    }

    #[test]
    fn lower_scoring_candidate_errors_are_ignored() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("hugo.toml"), "baseURL = '/'\n").unwrap();
        std::fs::create_dir(project.path().join("content")).unwrap();
        std::fs::create_dir(project.path().join("static")).unwrap();
        std::fs::write(project.path().join("index.php"), "<?php").unwrap();
        let (operation, _) = operation(IndexMap::from([(
            "ANYBUILD_PHP_ARCHITECTURE".to_owned(),
            "invalid".to_owned(),
        )]));

        let (kind, _) =
            select_provider(project.path(), &BaseConfig::default(), None, &operation).unwrap();

        assert_eq!(kind, ProviderKind::Hugo);
    }

    #[test]
    fn selected_candidate_errors_are_returned() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("index.php"), "<?php").unwrap();
        let (operation, _) = operation(IndexMap::from([(
            "ANYBUILD_PHP_ARCHITECTURE".to_owned(),
            "invalid".to_owned(),
        )]));

        let error =
            select_provider(project.path(), &BaseConfig::default(), None, &operation).unwrap_err();

        assert!(error.to_string().contains("ANYBUILD_PHP_ARCHITECTURE"));
    }

    #[test]
    fn explicit_provider_bypasses_other_candidates() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("index.php"), "<?php").unwrap();
        std::fs::write(project.path().join("index.html"), "<h1>Static</h1>").unwrap();
        let (operation, _) = operation(IndexMap::from([(
            "ANYBUILD_PHP_ARCHITECTURE".to_owned(),
            "invalid".to_owned(),
        )]));

        let (kind, _) = select_provider(
            project.path(),
            &BaseConfig::default(),
            Some("staticfile"),
            &operation,
        )
        .unwrap();

        assert_eq!(kind, ProviderKind::StaticFile);
    }

    #[test]
    fn selected_candidate_diagnostics_are_replayed_once() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("package.json"), "{}").unwrap();
        let (operation, events) = operation(IndexMap::new());

        let (kind, _) =
            select_provider(project.path(), &BaseConfig::default(), None, &operation).unwrap();

        assert_eq!(kind, ProviderKind::Node);
        let warning_count = events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    Event::Diagnostic {
                        level: DiagnosticLevel::Warning,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(warning_count, 2);
    }
}

#[cfg(test)]
mod config_inheritance_tests {
    use super::*;

    const BASE_FIELDS: &[&str] = &["name", "port", "commands", "services", "app_subdir"];
    const NODE_BUILD_FIELDS: &[&str] = &[
        "node_package_manager",
        "node_extra_dependencies",
        "node_build_command",
        "node_version",
        "npm_version",
        "pnpm_version",
        "yarn_version",
        "bun_version",
        "node_install_requires_all_files",
        "node_install_inputs",
        "node_package_name",
    ];
    const NODE_RUNTIME_FIELDS: &[&str] = &[
        "edgejs_enable",
        "edgejs_precompile",
        "node_framework",
        "node_server",
        "optimize_node_dependencies",
        "node_remove_native_binaries",
    ];
    const NODE_FIELDS: &[&str] = &[
        "edgejs_enable",
        "edgejs_precompile",
        "node_package_manager",
        "node_framework",
        "node_server",
        "node_extra_dependencies",
        "node_build_command",
        "node_version",
        "npm_version",
        "pnpm_version",
        "yarn_version",
        "bun_version",
        "optimize_node_dependencies",
        "node_remove_native_binaries",
        "node_install_requires_all_files",
        "node_install_inputs",
        "node_package_name",
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
                "static_convert_redirects",
                "sws_version",
                "static_dir",
                "static_redirects_config",
            ])
            .chain(NODE_FIELDS.iter().copied())
            .collect();
        let expected_laravel: Vec<&str> = BASE_FIELDS
            .iter()
            .copied()
            .chain(NODE_BUILD_FIELDS.iter().copied())
            .chain([
                "php_framework",
                "phpix",
                "composer_enable",
                "composer_build_script",
                "php_version",
                "php_architecture",
                "phpix_worker_threads",
                "php_public_dir",
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

    #[test]
    fn default_elision_omits_empty_collections_and_null_commands() {
        let mut config = ProviderConfig::Node(node::NodeConfig::default());
        config.base_mut().commands.start = Some("node server.js".to_owned());
        let ProviderConfig::Node(node) = &mut config else {
            unreachable!();
        };
        node.node.build.install_inputs = Some(Vec::new());

        assert_eq!(
            exclude_defaults_json(&config),
            serde_json::json!({"commands": {"start": "node server.js"}})
        );
    }

    #[test]
    fn laravel_excludes_node_runtime_fields() {
        let json = defaults_json("laravel").unwrap();
        let fields = json.as_object().unwrap();

        for field in NODE_RUNTIME_FIELDS {
            assert!(!fields.contains_key(*field), "{field}");
        }
    }

    #[test]
    fn every_registered_provider_round_trips_its_declared_defaults() {
        for kind in REGISTRY {
            let defaults = kind.defaults_json().unwrap();
            let config = kind.config_from_json(defaults.clone()).unwrap();

            assert_eq!(config.provider_name(), kind.name());
            assert_eq!(config.to_json(), defaults);
        }
    }
}
