//! Project detection and provider configuration.
//!
//! Provider registry and generation dispatch.
//! Each typed provider config implements `DetectableConfig`; selection scores
//! live here and registry order breaks ties.

use std::path::Path;

use anyhow::{anyhow, Result};

use crate::event::ProviderDetail;
use crate::operation::OperationContext;

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

pub use base::{BaseConfig, HasBase};

pub(crate) trait DetectableConfig: HasBase + Sized {
    type Evidence: Copy;

    fn detection_evidence(
        path: &Path,
        base: &BaseConfig,
        operation: &OperationContext,
    ) -> Option<Self::Evidence>;

    fn load(path: &Path, base: BaseConfig, operation: &OperationContext) -> Result<Self>;

    fn detect(
        path: &Path,
        base: &BaseConfig,
        operation: &OperationContext,
    ) -> Result<Option<Self>> {
        Self::detection_evidence(path, base, operation)
            .map(|_| Self::load(path, base.clone(), operation))
            .transpose()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderKind {
    Laravel,
    Hugo,
    Mkdocs,
    Python,
    Wordpress,
    Php,
    NodeStatic,
    Node,
    Jekyll,
    Go,
    StaticFile,
}

/// Order resolves equal scores and remains compatible with the original
/// provider registry.
const REGISTRY: &[ProviderKind] = &[
    ProviderKind::Laravel,
    ProviderKind::Hugo,
    ProviderKind::Mkdocs,
    ProviderKind::Python,
    ProviderKind::Wordpress,
    ProviderKind::Php,
    ProviderKind::NodeStatic,
    ProviderKind::Node,
    ProviderKind::Jekyll,
    ProviderKind::Go,
    ProviderKind::StaticFile,
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

    pub(crate) fn detection_details(&self) -> Vec<ProviderDetail> {
        let fields: &[(&str, &str)] = match self.provider_name() {
            "node" => &[
                ("Framework", "framework"),
                ("Package manager", "package_manager"),
                ("Node version", "node_version"),
            ],
            "node-static" => &[
                ("Framework", "framework"),
                ("Package manager", "package_manager"),
                ("Output directory", "static_dir"),
            ],
            "python" => &[
                ("Framework", "framework"),
                ("Server", "server"),
                ("Python version", "python_version"),
            ],
            "php" => &[("Framework", "framework"), ("PHP version", "php_version")],
            "wordpress" => &[
                ("Extension", "wp_extension_kind"),
                ("WordPress version", "wp_version"),
                ("PHP version", "php_version"),
            ],
            "laravel" => &[
                ("Package manager", "package_manager"),
                ("PHP version", "php_version"),
            ],
            "go" => &[
                ("Go version", "go_version"),
                ("Entrypoint", "go_build_file"),
            ],
            "staticfile" => &[
                ("Output directory", "static_dir"),
                ("SWS version", "sws_version"),
            ],
            "hugo" => &[
                ("Hugo version", "hugo_version"),
                ("Output directory", "static_dir"),
            ],
            "jekyll" => &[
                ("Jekyll version", "jekyll_version"),
                ("Ruby version", "ruby_version"),
                ("Output directory", "static_dir"),
            ],
            "mkdocs" => &[
                ("MkDocs version", "mkdocs_version"),
                ("Python version", "python_version"),
                ("Output directory", "static_dir"),
            ],
            _ => &[],
        };
        let config = self.to_json();
        let mut details: Vec<_> = fields
            .iter()
            .filter_map(|(label, field)| {
                let value = config
                    .get(field)?
                    .as_str()
                    .filter(|value| !value.is_empty())?;
                Some(ProviderDetail {
                    label: (*label).to_owned(),
                    value: display_detail(label, value),
                })
            })
            .collect();
        if self.provider_name() == "php"
            && config.get("use_composer").and_then(|value| value.as_bool()) == Some(true)
        {
            let index = usize::from(
                config
                    .get("framework")
                    .is_some_and(|value| !value.is_null()),
            );
            details.insert(
                index,
                ProviderDetail {
                    label: "Package manager".to_owned(),
                    value: "Composer".to_owned(),
                },
            );
        }
        details
    }

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

fn display_detail(label: &str, value: &str) -> String {
    if matches!(label, "Framework" | "Server" | "Extension") {
        display_config_value(value)
    } else {
        value.to_owned()
    }
}

fn display_config_value(value: &str) -> String {
    match value {
        "next" => "Next.js".to_owned(),
        "create-react-app" => "Create React App".to_owned(),
        "docusaurus-old" | "docusaurus" => "Docusaurus".to_owned(),
        "fastapi" => "FastAPI".to_owned(),
        "python-fasthtml" => "FastHTML".to_owned(),
        "mkdocs" => "MkDocs".to_owned(),
        "mcp" => "MCP".to_owned(),
        "node" => "Node.js".to_owned(),
        "npm" => "npm".to_owned(),
        "pnpm" => "pnpm".to_owned(),
        "umijs" => "UmiJS".to_owned(),
        "vitepress" => "VitePress".to_owned(),
        "vuepress" => "VuePress".to_owned(),
        "sveltekit" => "SvelteKit".to_owned(),
        "solidstart" => "SolidStart".to_owned(),
        "tanstack-start" => "TanStack Start".to_owned(),
        "react-router" => "React Router".to_owned(),
        "nuxt" | "nuxt3" => "Nuxt".to_owned(),
        "wordpress" => "WordPress".to_owned(),
        value if value.contains(['-', '_']) => value
            .split(['-', '_'])
            .filter(|part| !part.is_empty())
            .map(capitalize)
            .collect::<Vec<_>>()
            .join(" "),
        value => capitalize(value),
    }
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

fn detect_typed<C: DetectableConfig>(
    path: &Path,
    base: &BaseConfig,
    operation: &OperationContext,
    wrap: fn(C) -> ProviderConfig,
) -> Result<Option<ProviderConfig>> {
    C::detect(path, base, operation).map(|config| config.map(wrap))
}

fn load_typed<C: DetectableConfig>(
    path: &Path,
    base: &BaseConfig,
    operation: &OperationContext,
    wrap: fn(C) -> ProviderConfig,
) -> Result<ProviderConfig> {
    C::load(path, base.clone(), operation).map(wrap)
}

impl ProviderKind {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Laravel => laravel::NAME,
            Self::Hugo => hugo::NAME,
            Self::Mkdocs => mkdocs::NAME,
            Self::Python => "python",
            Self::Wordpress => wordpress::NAME,
            Self::Php => php::NAME,
            Self::NodeStatic => "node-static",
            Self::Node => "node",
            Self::Jekyll => jekyll::NAME,
            Self::Go => go::NAME,
            Self::StaticFile => staticfile::NAME,
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        REGISTRY
            .iter()
            .copied()
            .find(|kind| kind.name().eq_ignore_ascii_case(name))
    }

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
                php::DetectionEvidence::Entrypoint => 10,
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

    fn detect(
        self,
        path: &Path,
        base: &BaseConfig,
        operation: &OperationContext,
    ) -> Result<Option<ProviderConfig>> {
        match self {
            Self::Laravel => detect_typed(path, base, operation, ProviderConfig::Laravel),
            Self::Hugo => detect_typed(path, base, operation, ProviderConfig::Hugo),
            Self::Mkdocs => detect_typed(path, base, operation, ProviderConfig::Mkdocs),
            Self::Python => detect_typed(path, base, operation, ProviderConfig::Python),
            Self::Wordpress => detect_typed(path, base, operation, ProviderConfig::Wordpress),
            Self::Php => detect_typed(path, base, operation, ProviderConfig::Php),
            Self::NodeStatic => detect_typed(path, base, operation, ProviderConfig::NodeStatic),
            Self::Node => detect_typed(path, base, operation, ProviderConfig::Node),
            Self::Jekyll => detect_typed(path, base, operation, ProviderConfig::Jekyll),
            Self::Go => detect_typed(path, base, operation, ProviderConfig::Go),
            Self::StaticFile => detect_typed(path, base, operation, ProviderConfig::StaticFile),
        }
    }

    fn load(
        self,
        path: &Path,
        base: &BaseConfig,
        operation: &OperationContext,
    ) -> Result<ProviderConfig> {
        match self {
            Self::Python => load_typed(path, base, operation, ProviderConfig::Python),
            Self::Node => load_typed(path, base, operation, ProviderConfig::Node),
            Self::NodeStatic => load_typed(path, base, operation, ProviderConfig::NodeStatic),
            Self::Php => load_typed(path, base, operation, ProviderConfig::Php),
            Self::Wordpress => load_typed(path, base, operation, ProviderConfig::Wordpress),
            Self::Laravel => load_typed(path, base, operation, ProviderConfig::Laravel),
            Self::Go => load_typed(path, base, operation, ProviderConfig::Go),
            Self::StaticFile => load_typed(path, base, operation, ProviderConfig::StaticFile),
            Self::Hugo => load_typed(path, base, operation, ProviderConfig::Hugo),
            Self::Jekyll => load_typed(path, base, operation, ProviderConfig::Jekyll),
            Self::Mkdocs => load_typed(path, base, operation, ProviderConfig::Mkdocs),
        }
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

/// The declared field defaults for a provider config (pydantic's notion
/// for `exclude_defaults`). Defaults never apply the environment overlay.
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

/// Pydantic's `model_dump(mode="json", exclude_defaults=True)` for a typed
/// provider config.
pub fn exclude_defaults_json(config: &ProviderConfig) -> serde_json::Value {
    exclude_defaults_from_json(config.provider_name(), config.to_json())
}

/// Apply a provider's declared defaults to an already serialized config.
/// This is used by the Wasmer runner after applying runtime overrides.
pub fn exclude_defaults_from_json(name: &str, dumped: serde_json::Value) -> serde_json::Value {
    let Ok(serde_json::Value::Object(defaults)) = defaults_json(name) else {
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
