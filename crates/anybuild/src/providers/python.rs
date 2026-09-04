//! Python provider implementation.
//!
//! Serialization must match pydantic's `model_dump(mode="json")`: every
//! field present, `None` as null, enums as their value strings, sets as
//! sorted lists.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::operation::OperationContext;
use crate::providers::base::{
    env_bool, env_enum, env_json, env_str, is_blank, BaseConfig, DatabaseEngine, HasBase,
};
use crate::providers::install_context::{
    discover_python_dependency_files, discover_python_install_context,
};
use crate::providers::{humanize, Provider};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PythonFramework {
    #[serde(rename = "django")]
    Django,
    #[serde(rename = "streamlit")]
    Streamlit,
    #[serde(rename = "fastapi")]
    FastApi,
    #[serde(rename = "flask")]
    Flask,
    #[serde(rename = "python-fasthtml")]
    FastHtml,
    #[serde(rename = "mcp")]
    Mcp,
}

impl PythonFramework {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "django" => Some(Self::Django),
            "streamlit" => Some(Self::Streamlit),
            "fastapi" => Some(Self::FastApi),
            "flask" => Some(Self::Flask),
            "python-fasthtml" => Some(Self::FastHtml),
            "mcp" => Some(Self::Mcp),
            _ => None,
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::Django => "Django",
            Self::Streamlit => "Streamlit",
            Self::FastApi => "FastAPI",
            Self::Flask => "Flask",
            Self::FastHtml => "FastHTML",
            Self::Mcp => "MCP",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PythonServer {
    Hypercorn,
    Uvicorn,
    // Gunicorn intentionally absent (commented out in the Python enum).
    Daphne,
}

impl PythonServer {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "hypercorn" => Some(Self::Hypercorn),
            "uvicorn" => Some(Self::Uvicorn),
            "daphne" => Some(Self::Daphne),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MigrationStrategy {
    Django,
    Alembic,
}

impl MigrationStrategy {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "django" => Some(Self::Django),
            "alembic" => Some(Self::Alembic),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseType {
    MySql,
    PostgreSql,
}

impl DatabaseType {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "mysql" => Some(Self::MySql),
            "postgresql" => Some(Self::PostgreSql),
            _ => None,
        }
    }

    pub(crate) fn into_database_engine(self) -> DatabaseEngine {
        match self {
            Self::MySql => DatabaseEngine::Mysql,
            Self::PostgreSql => DatabaseEngine::Postgres,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PythonConfig {
    #[serde(flatten)]
    pub base: BaseConfig,
    #[serde(rename = "python_framework")]
    pub framework: Option<PythonFramework>,
    #[serde(rename = "python_server")]
    pub server: Option<PythonServer>,
    #[serde(rename = "python_migration_strategy")]
    pub migration_strategy: Option<MigrationStrategy>,
    #[serde(rename = "python_database")]
    pub database: Option<DatabaseType>,
    #[serde(rename = "python_extra_dependencies")]
    pub extra_dependencies: BTreeSet<String>,
    #[serde(rename = "asgi_application")]
    pub asgi_application: Option<String>,
    #[serde(rename = "wsgi_application")]
    pub wsgi_application: Option<String>,
    #[serde(rename = "python_install_requires_all_files")]
    pub install_requires_all_files: bool,
    #[serde(rename = "python_main_file")]
    pub main_file: Option<String>,
    pub python_version: Option<String>,
    pub uv_version: Option<String>,
    #[serde(rename = "python_precompile")]
    pub precompile_python: bool,
    #[serde(rename = "python_cross_platform")]
    pub cross_platform: Option<String>,
    pub python_extra_index_url: Option<String>,
    /// Derived install inputs for the Starlark provider (None => all files).
    #[serde(rename = "python_install_inputs")]
    pub install_inputs: Option<Vec<String>>,
    /// MCP main file starts its own server (mcp.run()/__main__ block).
    #[serde(rename = "python_mcp_self_running")]
    pub mcp_self_running: bool,
}

impl Default for PythonConfig {
    /// The declared pydantic field defaults (class-level defaults from
    /// python.py; no env overlay of the python fields — the env
    /// kill-switch for the base is the caller's concern).
    fn default() -> Self {
        Self {
            base: BaseConfig::default(),
            framework: None,
            server: None,
            migration_strategy: None,
            database: None,
            extra_dependencies: BTreeSet::new(),
            asgi_application: None,
            wsgi_application: None,
            install_requires_all_files: false,
            main_file: None,
            python_version: Some("3.13".to_owned()),
            uv_version: Some("0.8.15".to_owned()),
            precompile_python: true,
            cross_platform: None,
            python_extra_index_url: None,
            install_inputs: None,
            mcp_self_running: false,
        }
    }
}

impl PythonConfig {
    /// pydantic-settings construction: `PythonConfig(**base.model_dump())`.
    /// The base fields arrive already resolved; every python field reads
    /// its `ANYBUILD_<FIELD>` env var, falling back to the class default.
    fn from_base(base: BaseConfig, operation: &OperationContext) -> Result<Self> {
        Ok(Self {
            base,
            framework: env_enum(
                operation,
                "python_framework",
                "a supported Python framework",
                PythonFramework::parse,
            )?,
            server: env_enum(
                operation,
                "python_server",
                "a supported Python server",
                PythonServer::parse,
            )?,
            migration_strategy: env_enum(
                operation,
                "python_migration_strategy",
                "a supported migration strategy",
                MigrationStrategy::parse,
            )?,
            database: env_enum(
                operation,
                "python_database",
                "a supported database type",
                DatabaseType::parse,
            )?,
            extra_dependencies: env_json(operation, "python_extra_dependencies")?
                .unwrap_or_default(),
            asgi_application: env_str(operation, "asgi_application"),
            wsgi_application: env_str(operation, "wsgi_application"),
            install_requires_all_files: env_bool(operation, "python_install_requires_all_files")?
                .unwrap_or(false),
            main_file: env_str(operation, "python_main_file"),
            python_version: env_str(operation, "python_version")
                .or_else(|| Some("3.13".to_owned())),
            uv_version: env_str(operation, "uv_version").or_else(|| Some("0.8.15".to_owned())),
            precompile_python: env_bool(operation, "python_precompile")?.unwrap_or(true),
            cross_platform: env_str(operation, "python_cross_platform"),
            python_extra_index_url: env_str(operation, "python_extra_index_url"),
            install_inputs: env_json(operation, "python_install_inputs")?,
            mcp_self_running: env_bool(operation, "python_mcp_self_running")?.unwrap_or(false),
        })
    }
}

impl HasBase for PythonConfig {
    fn base(&self) -> &BaseConfig {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseConfig {
        &mut self.base
    }
}

pub const PG_DEPS: [&str; 6] = [
    "asyncpg",
    "aiopg",
    "psycopg",
    "psycopg2",
    "psycopg-binary",
    "psycopg2-binary",
];

pub const MYSQL_DEPS: [&str; 6] = [
    "mysqlclient",
    "pymysql",
    "mysql-connector-python",
    "aiomysql",
    "asyncmy",
    "mariadb",
];

/// Dependencies scanned to derive framework/server/tooling decisions.
pub const DEPENDENCY_SCAN: [&str; 14] = [
    "file://", // Not a dependency: signals that installs need all files.
    "streamlit",
    "alembic",
    "django",
    "mcp",
    "mcp[cli]",
    "fastapi",
    "flask",
    "python-fasthtml",
    "daphne",
    "hypercorn",
    "uvicorn",
    "ffmpeg",
    "pandoc",
];

fn exists(path: &Path, candidates: &[&str]) -> bool {
    candidates.iter().any(|c| path.join(c).exists())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DetectionEvidence {
    DjangoDependencies,
    Dependencies,
    Command,
    Entrypoint,
}

impl Provider for PythonConfig {
    type Evidence = DetectionEvidence;

    const NAME: &'static str = "python";
    const DETECTION_DETAILS: &'static [(&'static str, &'static str)] = &[
        ("Framework", "python_framework"),
        ("Server", "python_server"),
        ("Python version", "python_version"),
    ];

    fn format_detection_detail(field: &str, value: &str) -> String {
        match field {
            "python_framework" => PythonFramework::parse(value)
                .map(PythonFramework::display_name)
                .map(str::to_owned)
                .unwrap_or_else(|| humanize(value)),
            "python_server" => humanize(value),
            _ => value.to_owned(),
        }
    }

    fn detection_evidence(
        path: &Path,
        base: &BaseConfig,
        _operation: &OperationContext,
    ) -> Option<Self::Evidence> {
        if exists(path, &["pyproject.toml", "requirements.txt"]) {
            if exists(path, &["manage.py"]) {
                return Some(DetectionEvidence::DjangoDependencies);
            }
            return Some(DetectionEvidence::Dependencies);
        }
        if let Some(start) = base.commands.start.as_deref() {
            if start.starts_with("python ")
                || start.starts_with("uv ")
                || start.starts_with("uvicorn ")
                || start.starts_with("gunicorn ")
            {
                return Some(DetectionEvidence::Command);
            }
        }
        if detect_main_file(path).is_some() {
            return Some(DetectionEvidence::Entrypoint);
        }
        None
    }

    fn load(path: &Path, base: BaseConfig, operation: &OperationContext) -> Result<Self> {
        load_config(path, base, operation)
    }
}

/// Port of `PythonProvider.load_config`.
pub fn load_config(
    path: &Path,
    base: BaseConfig,
    operation: &OperationContext,
) -> Result<PythonConfig> {
    load_config_with_deps(path, base, BTreeSet::new(), operation)
}

/// `PythonProvider.load_config(path, base, must_have_deps=...)` — the
/// mkdocs provider passes `{"mkdocs"}`.
pub fn load_config_with_deps(
    path: &Path,
    base: BaseConfig,
    mut must_have_deps: BTreeSet<String>,
    operation: &OperationContext,
) -> Result<PythonConfig> {
    let mut config = PythonConfig::from_base(base, operation)?;

    if is_blank(&config.main_file) {
        config.main_file = detect_main_file(path);
    }

    if is_blank(&config.python_version) {
        let python_version = if exists(path, &[".python-version"]) {
            std::fs::read_to_string(path.join(".python-version"))
                .map(|text| text.trim().to_owned())
                .unwrap_or_else(|_| "3.13".to_owned())
        } else {
            "3.13".to_owned()
        };
        config.python_version = Some(python_version);
    }

    let scan: Vec<String> = DEPENDENCY_SCAN
        .iter()
        .chain(MYSQL_DEPS.iter())
        .chain(PG_DEPS.iter())
        .map(|s| (*s).to_owned())
        .chain(must_have_deps.iter().cloned())
        .collect();
    let found_deps = check_deps(path, scan);

    if requires_all_files(path, &found_deps) {
        config.install_requires_all_files = true;
    }

    if config.server.is_none() {
        config.server = detect_server(&found_deps);
    }

    if found_deps.contains("ffmpeg") {
        config.base.runtime_dependencies.push("ffmpeg".to_owned());
    }
    if found_deps.contains("pandoc") {
        config.base.runtime_dependencies.push("pandoc".to_owned());
    }

    if config.framework.is_none() {
        config.framework = detect_framework(path, &found_deps);
        if config.framework == Some(PythonFramework::Django) {
            // Known quirk (ported faithfully): overwrites a user-set
            // migration_strategy when Django is detected.
            config.migration_strategy = Some(MigrationStrategy::Django);
        }
    }

    if config.migration_strategy.is_none() {
        config.migration_strategy = detect_migration_strategy(path, config.framework, &found_deps);
    }

    if config.server.is_none() && config.framework.is_some() {
        config.server = default_server_for_framework(config.framework);
        if config.server == Some(PythonServer::Uvicorn) {
            must_have_deps.insert("uvicorn".to_owned());
        }
    }

    if is_blank(&config.asgi_application) && is_blank(&config.wsgi_application) {
        let (asgi, wsgi) =
            resolve_applications(path, config.framework, config.main_file.as_deref());
        config.asgi_application = asgi;
        config.wsgi_application = wsgi;
    }

    let is_uvicorn_start = config
        .base
        .commands
        .start
        .as_deref()
        .is_some_and(|start| start.starts_with("uvicorn "));
    let framework_should_use_uvicorn = matches!(
        config.framework,
        Some(PythonFramework::Django | PythonFramework::FastApi | PythonFramework::Flask)
    );
    if is_uvicorn_start || (framework_should_use_uvicorn && config.server.is_none()) {
        must_have_deps.insert("uvicorn".to_owned());
        config.server = Some(PythonServer::Uvicorn);
    }
    if config.framework == Some(PythonFramework::Mcp) {
        must_have_deps.insert("mcp[cli]".to_owned());
    }

    for dep in &must_have_deps {
        if !found_deps.contains(dep) {
            config.extra_dependencies.insert(dep.clone());
        }
    }

    let detected_database = config
        .database
        .map(DatabaseType::into_database_engine)
        .or_else(|| detect_database(&found_deps));
    if let Some(database) = detected_database {
        config.base.set_database_service(database);
    }

    config.install_inputs = compute_install_inputs(path);
    config.mcp_self_running =
        detect_mcp_self_running(path, config.framework, config.main_file.as_deref());

    if is_blank(&config.base.commands.start) {
        config.base.commands.start = infer_start_command(&config, true, operation);
    }

    Ok(config)
}

fn cannot_infer_start(reason: &str, warn: bool, operation: &OperationContext) -> Option<String> {
    if warn {
        operation.emit(crate::event::Event::Diagnostic {
            level: crate::event::DiagnosticLevel::Warning,
            message: reason.to_owned(),
        });
    }
    None
}

fn missing_main_file(
    config: &PythonConfig,
    warn: bool,
    operation: &OperationContext,
) -> Option<String> {
    match config.framework {
        Some(framework) => cannot_infer_start(
            &format!(
                "no main file was detected for the {} framework",
                framework.display_name()
            ),
            warn,
            operation,
        ),
        None => cannot_infer_start(
            "no main file was detected for Python project",
            warn,
            operation,
        ),
    }
}

/// Resolve the provider-level start command before Starlark evaluation.
pub(crate) fn infer_start_command(
    config: &PythonConfig,
    warn: bool,
    operation: &OperationContext,
) -> Option<String> {
    let main_file = config
        .main_file
        .as_deref()
        .filter(|value| !value.is_empty());
    let asgi = config
        .asgi_application
        .as_deref()
        .filter(|value| !value.is_empty());
    let wsgi = config
        .wsgi_application
        .as_deref()
        .filter(|value| !value.is_empty());

    if config.server == Some(PythonServer::Daphne) {
        return match asgi {
            Some(asgi) => Some(format!("daphne {asgi} --bind 0.0.0.0 --port $PORT")),
            None => cannot_infer_start(
                "no ASGI application was detected for the Daphne server",
                warn,
                operation,
            ),
        };
    }

    if config.server == Some(PythonServer::Uvicorn) {
        if let Some(asgi) = asgi {
            return Some(format!("uvicorn {asgi} --host 0.0.0.0 --port $PORT"));
        }
        if let Some(wsgi) = wsgi {
            return Some(format!(
                "uvicorn {wsgi} --interface=wsgi --host 0.0.0.0 --port $PORT"
            ));
        }
        if main_file.is_none() {
            if config.framework.is_some() {
                return missing_main_file(config, warn, operation);
            }
            return cannot_infer_start(
                "no main file, ASGI application, or WSGI application was detected \
                 for the Uvicorn server",
                warn,
                operation,
            );
        }
    } else if config.server == Some(PythonServer::Hypercorn) {
        return match asgi {
            Some(asgi) => Some(format!("hypercorn {asgi} --bind 0.0.0.0:$PORT")),
            None => cannot_infer_start(
                "no ASGI application was detected for the Hypercorn server",
                warn,
                operation,
            ),
        };
    } else if config.framework == Some(PythonFramework::Streamlit) {
        return match main_file {
            Some(main_file) => Some(format!(
                "streamlit run {main_file} --server.port $PORT \
                 --server.address 0.0.0.0 --server.headless true"
            )),
            None => missing_main_file(config, warn, operation),
        };
    } else if config.framework == Some(PythonFramework::Mcp) {
        let Some(main_file) = main_file else {
            return missing_main_file(config, warn, operation);
        };
        return if config.mcp_self_running {
            Some(format!("python {main_file}"))
        } else {
            Some(format!(
                "python $VIRTUAL_ENV/bin/mcp run {main_file} \
                 --transport=streamable-http"
            ))
        };
    } else if config.framework == Some(PythonFramework::Django) {
        return Some("python manage.py runserver 0.0.0.0:$PORT".to_owned());
    }

    match main_file {
        Some(main_file) => Some(format!("python {main_file}")),
        None => missing_main_file(config, warn, operation),
    }
}

/// Port of `PythonProvider.check_deps`: substring scan over the
/// discovered dependency files (uv.lock excluded).
fn check_deps(path: &Path, deps: impl IntoIterator<Item = String>) -> BTreeSet<String> {
    let initial: BTreeSet<String> = deps.into_iter().map(|dep| dep.to_lowercase()).collect();
    let mut pending = initial.clone();
    'files: for file in discover_python_dependency_files(path) {
        if file.file_name().is_some_and(|name| name == "uv.lock") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&file) else {
            continue;
        };
        let contents = String::from_utf8_lossy(&bytes);
        for line in contents.lines() {
            let line = line.to_lowercase();
            pending.retain(|dep| !line.contains(dep.as_str()));
            if pending.is_empty() {
                break 'files;
            }
        }
    }
    initial.difference(&pending).cloned().collect()
}

/// Port of `PythonProvider.detect_main_file`.
fn detect_main_file(root_path: &Path) -> Option<String> {
    const PATHS_TO_TRY: [&str; 5] = [
        "main.py",
        "app.py",
        "streamlit_app.py",
        "Home.py",
        "*_app.py",
    ];
    for candidate in PATHS_TO_TRY {
        if candidate.contains('*') {
            continue; // This is for the glob finder.
        }
        if root_path.join(candidate).exists() {
            return Some(candidate.to_owned());
        }
        if root_path.join("src").join(candidate).exists() {
            return Some(format!("src/{candidate}"));
        }
    }
    for candidate in PATHS_TO_TRY {
        let found = glob_at_depth(root_path, 1, candidate, "")
            .or_else(|| glob_at_depth(root_path, 2, candidate, ""));
        if found.is_some() {
            return found;
        }
    }
    None
}

/// First match for `*/.../<name_pattern>` (`depth` directory wildcards),
/// scanning directories in sorted order.
fn glob_at_depth(dir: &Path, depth: usize, name_pattern: &str, rel: &str) -> Option<String> {
    let mut entries: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                entry.path(),
            )
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    if depth == 0 {
        for (name, _) in &entries {
            if matches_name(name_pattern, name) {
                return Some(join_rel(rel, name));
            }
        }
        return None;
    }
    for (name, path) in &entries {
        if path.is_dir() {
            if let Some(found) = glob_at_depth(path, depth - 1, name_pattern, &join_rel(rel, name))
            {
                return Some(found);
            }
        }
    }
    None
}

fn join_rel(rel: &str, name: &str) -> String {
    if rel.is_empty() {
        name.to_owned()
    } else {
        format!("{rel}/{name}")
    }
}

fn matches_name(pattern: &str, name: &str) -> bool {
    match pattern.strip_prefix('*') {
        Some(suffix) => name.ends_with(suffix),
        None => pattern == name,
    }
}

/// Port of `format_app_import`: "mysite.asgi.application" ->
/// "mysite.asgi:application".
fn format_app_import(application: &str) -> String {
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\.([^.]+)$").unwrap());
    RE.replace(application, ":$1").into_owned()
}

/// Port of `file_to_python_path` (including the `str.rstrip(".py")`
/// character-set quirk).
fn file_to_python_path(path: Option<&str>) -> Option<String> {
    let path = path?;
    if path.is_empty() {
        return None;
    }
    let file = path
        .trim_end_matches(['.', 'p', 'y'])
        .replace(['/', '\\'], ".");
    Some(format!("{file}:app"))
}

/// Port of `_requires_all_files`: installs that reference local files
/// need the whole tree staged.
fn requires_all_files(path: &Path, found_deps: &BTreeSet<String>) -> bool {
    if found_deps.contains("file://") {
        return true;
    }
    discover_python_install_context(
        path,
        exists(path, &["pyproject.toml"]),
        exists(path, &["requirements.txt"]),
    )
    .requires_all_files
}

/// Port of `_detect_server`.
fn detect_server(found_deps: &BTreeSet<String>) -> Option<PythonServer> {
    if found_deps.contains("uvicorn") {
        return Some(PythonServer::Uvicorn);
    }
    if found_deps.contains("hypercorn") {
        return Some(PythonServer::Hypercorn);
    }
    if found_deps.contains("daphne") {
        return Some(PythonServer::Daphne);
    }
    None
}

/// Port of `_detect_framework`.
fn detect_framework(path: &Path, found_deps: &BTreeSet<String>) -> Option<PythonFramework> {
    if exists(path, &["manage.py"]) && found_deps.contains("django") {
        return Some(PythonFramework::Django);
    }
    if found_deps.contains("streamlit") {
        return Some(PythonFramework::Streamlit);
    }
    if found_deps.contains("mcp") {
        return Some(PythonFramework::Mcp);
    }
    if found_deps.contains("fastapi") {
        return Some(PythonFramework::FastApi);
    }
    if found_deps.contains("flask") {
        return Some(PythonFramework::Flask);
    }
    if found_deps.contains("python-fasthtml") {
        return Some(PythonFramework::FastHtml);
    }
    None
}

/// Port of `_detect_migration_strategy`.
fn detect_migration_strategy(
    path: &Path,
    framework: Option<PythonFramework>,
    found_deps: &BTreeSet<String>,
) -> Option<MigrationStrategy> {
    if framework == Some(PythonFramework::Django) {
        return Some(MigrationStrategy::Django);
    }
    if found_deps.contains("alembic") || exists(path, &["alembic.ini"]) {
        return Some(MigrationStrategy::Alembic);
    }
    None
}

/// Port of `_default_server_for_framework`.
fn default_server_for_framework(framework: Option<PythonFramework>) -> Option<PythonServer> {
    match framework {
        Some(
            PythonFramework::Django
            | PythonFramework::FastApi
            | PythonFramework::Flask
            | PythonFramework::FastHtml,
        ) => Some(PythonServer::Uvicorn),
        _ => None,
    }
}

/// Port of `_resolve_applications`: locate the (asgi, wsgi) application
/// imports for the framework.
fn resolve_applications(
    path: &Path,
    framework: Option<PythonFramework>,
    main_file: Option<&str>,
) -> (Option<String>, Option<String>) {
    static ASGI_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"ASGI_APPLICATION\s*=\s*['"](.*)['"]"#).unwrap());
    static WSGI_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"WSGI_APPLICATION\s*=\s*['"](.*)['"]"#).unwrap());

    let mut asgi: Option<String> = None;
    let mut wsgi: Option<String> = None;
    if framework == Some(PythonFramework::Django) {
        if let Some(settings_file) = find_first(path, "settings.py") {
            let settings = std::fs::read_to_string(&settings_file).unwrap_or_default();
            if let Some(captures) = ASGI_RE.captures(&settings) {
                asgi = Some(format_app_import(captures.get(1).unwrap().as_str()));
            } else if let Some(captures) = WSGI_RE.captures(&settings) {
                wsgi = Some(format_app_import(captures.get(1).unwrap().as_str()));
            }
        }
    }

    let python_path = file_to_python_path(main_file);
    match framework {
        Some(PythonFramework::FastApi) => asgi = python_path,
        Some(PythonFramework::Flask) => wsgi = python_path,
        Some(PythonFramework::Mcp) => asgi = python_path,
        Some(PythonFramework::FastHtml) => asgi = python_path,
        _ => {}
    }
    (asgi, wsgi)
}

/// `next(path.glob(f"**/{name}"), None)`.
fn find_first(path: &Path, name: &str) -> Option<PathBuf> {
    walkdir::WalkDir::new(path)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|entry| entry.ok())
        .find(|entry| entry.file_name() == name)
        .map(|entry| entry.into_path())
}

/// Port of `_detect_database`.
fn detect_database(found_deps: &BTreeSet<String>) -> Option<DatabaseEngine> {
    if MYSQL_DEPS.iter().any(|dep| found_deps.contains(*dep)) {
        Some(DatabaseEngine::Mysql)
    } else if PG_DEPS.iter().any(|dep| found_deps.contains(*dep)) {
        Some(DatabaseEngine::Postgres)
    } else {
        None
    }
}

/// Port of `_compute_install_inputs`.
fn compute_install_inputs(path: &Path) -> Option<Vec<String>> {
    if exists(path, &["pyproject.toml"]) {
        return Some(discover_python_install_context(path, true, false).inputs);
    }
    Some(discover_python_install_context(path, false, exists(path, &["requirements.txt"])).inputs)
}

/// Port of `_detect_mcp_self_running`: whether the MCP main file starts
/// its own server.
fn detect_mcp_self_running(
    path: &Path,
    framework: Option<PythonFramework>,
    main_file: Option<&str>,
) -> bool {
    if framework != Some(PythonFramework::Mcp) {
        return false;
    }
    let Some(main_file) = main_file.filter(|f| !f.is_empty()) else {
        return false;
    };
    let main_path = path.join(main_file);
    if !main_path.is_file() {
        return false;
    }
    let Ok(contents) = std::fs::read_to_string(&main_path) else {
        return false;
    };
    contents.contains("if __name__ == \"__main__\"") || contents.contains("mcp.run")
}

#[cfg(test)]
mod tests {
    //! Port of `tests/test_python_detection.py`.

    use super::*;

    fn load_config(path: &Path, base: BaseConfig) -> Result<PythonConfig> {
        super::load_config(path, base, &OperationContext::for_test())
    }

    fn infer_start_command(config: &PythonConfig, warn: bool) -> Option<String> {
        super::infer_start_command(config, warn, &OperationContext::for_test())
    }

    fn deps(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn test_detect_server_precedence() {
        assert_eq!(
            detect_server(&deps(&["uvicorn", "daphne"])),
            Some(PythonServer::Uvicorn)
        );
        assert_eq!(
            detect_server(&deps(&["hypercorn", "daphne"])),
            Some(PythonServer::Hypercorn)
        );
        assert_eq!(
            detect_server(&deps(&["daphne"])),
            Some(PythonServer::Daphne)
        );
        assert_eq!(detect_server(&deps(&["flask"])), None);
    }

    #[test]
    fn test_detect_framework_precedence() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();
        // Django requires manage.py in addition to the dependency.
        assert_eq!(
            detect_framework(path, &deps(&["django", "fastapi"])),
            Some(PythonFramework::FastApi)
        );
        std::fs::write(path.join("manage.py"), "").unwrap();
        assert_eq!(
            detect_framework(path, &deps(&["django", "fastapi"])),
            Some(PythonFramework::Django)
        );
        // Streamlit beats MCP beats FastAPI beats Flask beats FastHTML.
        assert_eq!(
            detect_framework(path, &deps(&["streamlit", "mcp"])),
            Some(PythonFramework::Streamlit)
        );
        assert_eq!(
            detect_framework(path, &deps(&["mcp", "fastapi"])),
            Some(PythonFramework::Mcp)
        );
        assert_eq!(
            detect_framework(path, &deps(&["flask", "python-fasthtml"])),
            Some(PythonFramework::Flask)
        );
        assert_eq!(detect_framework(path, &deps(&[])), None);
    }

    #[test]
    fn test_detect_migration_strategy() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();
        assert_eq!(
            detect_migration_strategy(path, Some(PythonFramework::Django), &deps(&[])),
            Some(MigrationStrategy::Django)
        );
        assert_eq!(
            detect_migration_strategy(path, None, &deps(&["alembic"])),
            Some(MigrationStrategy::Alembic)
        );
        std::fs::write(path.join("alembic.ini"), "").unwrap();
        assert_eq!(
            detect_migration_strategy(path, None, &deps(&[])),
            Some(MigrationStrategy::Alembic)
        );
    }

    #[test]
    fn test_default_server_for_framework() {
        assert_eq!(
            default_server_for_framework(Some(PythonFramework::Django)),
            Some(PythonServer::Uvicorn)
        );
        assert_eq!(
            default_server_for_framework(Some(PythonFramework::FastHtml)),
            Some(PythonServer::Uvicorn)
        );
        assert_eq!(
            default_server_for_framework(Some(PythonFramework::Streamlit)),
            None
        );
        assert_eq!(default_server_for_framework(None), None);
    }

    #[test]
    fn test_resolve_applications_django_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();
        let settings = path.join("mysite").join("settings.py");
        std::fs::create_dir(settings.parent().unwrap()).unwrap();
        std::fs::write(
            &settings,
            "WSGI_APPLICATION = \"mysite.wsgi.application\"\n",
        )
        .unwrap();

        let (asgi, wsgi) = resolve_applications(path, Some(PythonFramework::Django), None);
        assert_eq!(asgi, None);
        assert_eq!(wsgi.as_deref(), Some("mysite.wsgi:application"));

        std::fs::write(
            &settings,
            "ASGI_APPLICATION = \"mysite.asgi.application\"\n",
        )
        .unwrap();
        let (asgi, wsgi) = resolve_applications(path, Some(PythonFramework::Django), None);
        assert_eq!(asgi.as_deref(), Some("mysite.asgi:application"));
        assert_eq!(wsgi, None);
    }

    #[test]
    fn test_resolve_applications_from_main_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();
        let (asgi, wsgi) =
            resolve_applications(path, Some(PythonFramework::FastApi), Some("src/main.py"));
        assert_eq!(
            (asgi.as_deref(), wsgi.as_deref()),
            (Some("src.main:app"), None)
        );
        let (asgi, wsgi) =
            resolve_applications(path, Some(PythonFramework::Flask), Some("main.py"));
        assert_eq!((asgi.as_deref(), wsgi.as_deref()), (None, Some("main:app")));
    }

    #[test]
    fn test_detect_database_clients_with_mysql_precedence() {
        assert_eq!(
            detect_database(&deps(&["pymysql"])),
            Some(DatabaseEngine::Mysql)
        );
        assert_eq!(
            detect_database(&deps(&["asyncpg"])),
            Some(DatabaseEngine::Postgres)
        );
        assert_eq!(
            detect_database(&deps(&["pymysql", "psycopg"])),
            Some(DatabaseEngine::Mysql)
        );
        assert_eq!(detect_database(&deps(&["flask"])), None);
    }

    #[test]
    fn test_database_detection_populates_common_services() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("requirements.txt"), "psycopg\n").unwrap();

        let config = load_config(tmp.path(), BaseConfig::default()).unwrap();

        assert_eq!(
            config.base.services,
            vec![crate::providers::base::ServiceConfig::database(
                DatabaseEngine::Postgres
            )]
        );
        assert_eq!(config.database, None);
    }

    #[test]
    fn test_legacy_database_field_populates_common_services() {
        let tmp = tempfile::tempdir().unwrap();
        let config = crate::providers::finalize_config(
            tmp.path(),
            crate::providers::ProviderConfig::Python(PythonConfig {
                database: Some(DatabaseType::PostgreSql),
                ..PythonConfig::default()
            }),
        );
        let crate::providers::ProviderConfig::Python(config) = config else {
            panic!("expected python config");
        };

        assert_eq!(
            config.base.services,
            vec![crate::providers::base::ServiceConfig::database(
                DatabaseEngine::Postgres
            )]
        );
    }

    #[test]
    fn test_detect_mcp_self_running() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();
        std::fs::write(path.join("main.py"), "mcp.run()\n").unwrap();
        assert!(detect_mcp_self_running(
            path,
            Some(PythonFramework::Mcp),
            Some("main.py")
        ));
        std::fs::write(path.join("main.py"), "app = build_server()\n").unwrap();
        assert!(!detect_mcp_self_running(
            path,
            Some(PythonFramework::Mcp),
            Some("main.py")
        ));
        // Only applies to MCP projects with a resolvable main file.
        assert!(!detect_mcp_self_running(
            path,
            Some(PythonFramework::FastApi),
            Some("main.py")
        ));
        assert!(!detect_mcp_self_running(
            path,
            Some(PythonFramework::Mcp),
            None
        ));
    }

    #[test]
    fn test_python_provider_infers_start_command() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("pyproject.toml"),
            "[project]\nname = 'app'\n",
        )
        .unwrap();
        std::fs::write(tmp.path().join("main.py"), "print('hello')\n").unwrap();

        let config = load_config(tmp.path(), BaseConfig::default()).unwrap();

        assert_eq!(
            config.base.commands.start.as_deref(),
            Some("python main.py")
        );
    }

    #[test]
    fn test_infer_start_command_for_servers_and_frameworks() {
        let mut config = PythonConfig {
            server: Some(PythonServer::Daphne),
            asgi_application: Some("app:api".to_owned()),
            ..PythonConfig::default()
        };
        assert_eq!(
            infer_start_command(&config, false).as_deref(),
            Some("daphne app:api --bind 0.0.0.0 --port $PORT")
        );

        config.server = Some(PythonServer::Uvicorn);
        assert_eq!(
            infer_start_command(&config, false).as_deref(),
            Some("uvicorn app:api --host 0.0.0.0 --port $PORT")
        );

        config.asgi_application = None;
        config.wsgi_application = Some("app:web".to_owned());
        assert_eq!(
            infer_start_command(&config, false).as_deref(),
            Some("uvicorn app:web --interface=wsgi --host 0.0.0.0 --port $PORT")
        );

        config.server = Some(PythonServer::Hypercorn);
        config.asgi_application = Some("app:api".to_owned());
        config.wsgi_application = None;
        assert_eq!(
            infer_start_command(&config, false).as_deref(),
            Some("hypercorn app:api --bind 0.0.0.0:$PORT")
        );

        config.server = None;
        config.asgi_application = None;
        config.framework = Some(PythonFramework::Streamlit);
        config.main_file = Some("streamlit_app.py".to_owned());
        assert_eq!(
            infer_start_command(&config, false).as_deref(),
            Some(
                "streamlit run streamlit_app.py --server.port $PORT \
                 --server.address 0.0.0.0 --server.headless true"
            )
        );

        config.framework = Some(PythonFramework::Mcp);
        config.main_file = Some("main.py".to_owned());
        assert_eq!(
            infer_start_command(&config, false).as_deref(),
            Some(
                "python $VIRTUAL_ENV/bin/mcp run main.py \
                 --transport=streamable-http"
            )
        );

        config.framework = Some(PythonFramework::Django);
        config.main_file = None;
        assert_eq!(
            infer_start_command(&config, false).as_deref(),
            Some("python manage.py runserver 0.0.0.0:$PORT")
        );
    }

    #[test]
    fn test_infer_start_command_reports_missing_evidence() {
        for config in [
            PythonConfig {
                framework: Some(PythonFramework::Streamlit),
                ..PythonConfig::default()
            },
            PythonConfig {
                framework: Some(PythonFramework::Mcp),
                ..PythonConfig::default()
            },
            PythonConfig {
                server: Some(PythonServer::Daphne),
                ..PythonConfig::default()
            },
            PythonConfig {
                server: Some(PythonServer::Hypercorn),
                ..PythonConfig::default()
            },
            PythonConfig {
                server: Some(PythonServer::Uvicorn),
                ..PythonConfig::default()
            },
        ] {
            assert_eq!(infer_start_command(&config, false), None);
        }
    }
}
