//! Wasmer runtime implementation.
//!
//! Two invariants carried over verbatim (see plans/rust-migration.md):
//!
//! - `prepare_config` applies the Wasmer-specific Python, PHP, and Node
//!   overrides before plan evaluation so the generated dependencies and
//!   commands agree with the selected runner.
//! - `resolve_app_kind` keys off `serve.provider` (serve-side identity).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::LazyLock;

use anyhow::{anyhow, bail, ensure, Context, Result};
use indexmap::IndexMap;
use serde_json::Value as JsonValue;
use serde_yaml::Value as YamlValue;
use sha2::Digest;
use toml_edit::{value, Array, ArrayOfTables, DocumentMut, Item, Table};

use crate::build::BuildBackend;
use crate::common::volumes::load_volume_mappings;
use crate::event::{Event, WasmerPackageMapping};
use crate::operation::OperationContext;
use crate::plan::{EnvStep, Package, RunStep, Serve, Step, UseStep};
use crate::providers::ProviderConfig;

use crate::run::{HostMount, Runner};

pub const ANYBUILD_CONFIG_ANNOTATION: &str = "anybuild.run/config";
pub const ANYBUILD_PROVIDER_ANNOTATION: &str = "anybuild.run/provider";
pub const ANYBUILD_VERSION_ANNOTATION: &str = "anybuild.run/version";
pub const WASMER_APP_KIND_ANNOTATION: &str = "wasmer.io/app-kind";
pub const WASMER_VERSION_ANNOTATION: &str = "wasmer.io/version";
pub const BUILD_ANNOTATIONS_FILENAME: &str = "build-annotations.yaml";
pub const EDGEJS_QUICKJS_DEPENDENCY: &str = "wasmer/edgejs-quickjs@=0.1.0";
pub const PHPIX_VERSION: &str = "0.3.0-rc.3";
const PREPARE_COMMAND_PREFIX: &str = "__anybuild_prepare_";

/// The workspace package version is embedded in every Rust crate and kept in
/// sync with the Python package by release-please.
pub fn anybuild_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Port of `_phpix_dependency`.
fn phpix_dependency(php_version: &str, bits: u32) -> String {
    format!("phpix/phpix-{php_version}-{bits}bit@={PHPIX_VERSION}")
}

/// Port of `serialize_provider_config`: pydantic's
/// `model_dump(mode="json", exclude_defaults=True)` over the runner-side
/// config JSON.
pub fn serialize_provider_config(provider: &str, provider_config: Option<&JsonValue>) -> JsonValue {
    let dumped = provider_config
        .cloned()
        .unwrap_or_else(|| JsonValue::Object(serde_json::Map::new()));
    crate::providers::exclude_defaults_from_json(provider, dumped)
}

/// Port of `resolve_app_kind`. Keys off the *serve-side* provider
/// identity (deliberate — see module docs).
pub fn resolve_app_kind(provider: &str, framework: Option<&str>) -> Option<String> {
    if provider == "wordpress" {
        return Some("wordpress".to_owned());
    }
    let framework = framework?;
    let provider_name = match provider {
        "node" | "node-static" => "javascript",
        other => other,
    };
    let framework_name = framework.to_lowercase();
    let app_kinds: &[&str] = match provider_name {
        "python" => &["django", "mcp"],
        "php" => &["moodle", "drupal"],
        "javascript" => &["ghost", "strapi"],
        _ => &[],
    };
    if app_kinds.contains(&framework_name.as_str()) {
        Some(framework_name)
    } else {
        None
    }
}

/// Port of the `MapperItem` TypedDict.
pub struct MapperItem {
    pub dependencies: IndexMap<&'static str, String>,
    pub scripts: &'static [&'static str],
    /// `None` mirrors a missing "env" key (pandoc/ffmpeg).
    pub env: Option<&'static [(&'static str, &'static str)]>,
    pub aliases: &'static [(&'static str, &'static str)],
    pub architecture_dependencies: IndexMap<&'static str, IndexMap<&'static str, String>>,
}

fn deps(pairs: &[(&'static str, &str)]) -> IndexMap<&'static str, String> {
    pairs.iter().map(|(k, v)| (*k, (*v).to_owned())).collect()
}

/// Port of `WasmerRunner.rewrite_binaries`.
fn rewrite_binary(program: &str) -> Option<&'static str> {
    Some(match program {
        "python" => "python",
        "python3" => "python",
        "python3.13" => "python",
        "daphne" => "python -m daphne",
        "gunicorn" => "python -m gunicorn",
        "uvicorn" => "python -m uvicorn",
        "alembic" => "python -m alembic",
        "hypercorn" => "python -m hypercorn",
        "fastapi" => "python -m fastapi",
        "streamlit" => "python -m streamlit",
        "next" => "node node_modules/.bin/next",
        "nuxt" => "node node_modules/.bin/nuxt",
        "gatsby" => "node node_modules/.bin/gatsby",
        "svelte" => "node node_modules/.bin/svelte",
        "remix" => "node node_modules/.bin/remix",
        "astro" => "node node_modules/.bin/astro",
        "vite" => "node node_modules/.bin/vite",
        "hexo" => "node node_modules/.bin/hexo",
        "flask" => "python -m flask",
        "mcp" => "python -m mcp",
        "node" => "edge",
        _ => return None,
    })
}

/// Port of `WasmerRunner.mapper`.
pub fn mapper() -> &'static IndexMap<&'static str, MapperItem> {
    static MAPPER: LazyLock<IndexMap<&'static str, MapperItem>> = LazyLock::new(|| {
        let mut map = IndexMap::new();
        map.insert(
            "node",
            MapperItem {
                dependencies: deps(&[
                    ("latest", EDGEJS_QUICKJS_DEPENDENCY),
                    ("24", EDGEJS_QUICKJS_DEPENDENCY),
                    ("22", EDGEJS_QUICKJS_DEPENDENCY),
                ]),
                scripts: &["edge"],
                env: Some(&[]),
                aliases: &[("node", "edge"), ("edgejs", "edge")],
                architecture_dependencies: IndexMap::new(),
            },
        );
        map.insert(
            "python",
            MapperItem {
                dependencies: deps(&[
                    ("latest", "python/python@=3.13.5"),
                    ("3.13", "python/python@=3.13.5"),
                ]),
                scripts: &["python"],
                env: Some(&[("PYTHONEXECUTABLE", "/bin/python")]),
                aliases: &[],
                architecture_dependencies: IndexMap::new(),
            },
        );
        map.insert(
            "pandoc",
            MapperItem {
                dependencies: deps(&[
                    ("latest", "wasmer/pandoc@=0.0.1"),
                    ("3.5", "wasmer/pandoc@=0.0.1"),
                ]),
                scripts: &["pandoc"],
                env: None,
                aliases: &[],
                architecture_dependencies: IndexMap::new(),
            },
        );
        map.insert(
            "ffmpeg",
            MapperItem {
                dependencies: deps(&[
                    ("latest", "wasmer/ffmpeg@=1.0.5"),
                    ("N-111519", "wasmer/ffmpeg@=1.0.5"),
                ]),
                scripts: &["ffmpeg"],
                env: None,
                aliases: &[],
                architecture_dependencies: IndexMap::new(),
            },
        );
        let php_versions = |bits: &str| {
            deps(&[
                ("latest", &format!("php/php-{bits}@=8.3.2102")),
                ("8.3", &format!("php/php-{bits}@=8.3.2102")),
                ("8.3.29", &format!("php/php-{bits}@=8.3.2102")),
                ("8.2", &format!("php/php-{bits}@=8.2.2801")),
                ("8.1", &format!("php/php-{bits}@=8.1.3201")),
                ("7.4", &format!("php/php-{bits}@=7.4.3301")),
            ])
        };
        map.insert(
            "php",
            MapperItem {
                dependencies: php_versions("32"),
                scripts: &["php"],
                env: Some(&[]),
                aliases: &[],
                architecture_dependencies: [
                    ("64-bit", php_versions("64")),
                    ("32-bit", php_versions("32")),
                ]
                .into_iter()
                .collect(),
            },
        );
        let phpix_versions = |bits: u32| {
            deps(&[
                ("latest", &phpix_dependency("84", bits)),
                ("8.5", &phpix_dependency("85", bits)),
                ("8.4", &phpix_dependency("84", bits)),
                ("8.3", &phpix_dependency("83", bits)),
                ("8.3.29", &phpix_dependency("83", bits)),
                ("8.2", &phpix_dependency("82", bits)),
                ("8.1", &phpix_dependency("81", bits)),
                // Note, we don't have PHPix + PHP 7.4 and never will.
                ("7.4", &format!("php/php-{bits}@=7.4.3301")),
            ])
        };
        map.insert(
            "phpix",
            MapperItem {
                dependencies: phpix_versions(32),
                scripts: &["php", "phpix"],
                env: Some(&[]),
                aliases: &[("php", "phpix")],
                architecture_dependencies: [
                    ("64-bit", phpix_versions(64)),
                    ("32-bit", phpix_versions(32)),
                ]
                .into_iter()
                .collect(),
            },
        );
        map.insert(
            "bash",
            MapperItem {
                dependencies: deps(&[
                    ("latest", "wasmer/bash@=1.0.25"),
                    ("8.3", "wasmer/bash@=1.0.25"),
                ]),
                scripts: &["bash", "sh"],
                env: Some(&[]),
                aliases: &[],
                architecture_dependencies: IndexMap::new(),
            },
        );
        map.insert(
            "static-web-server",
            MapperItem {
                dependencies: deps(&[
                    ("latest", "wasmer/static-web-server@=1.1.0"),
                    ("2.38.0", "wasmer/static-web-server@=1.1.0"),
                    ("0.1", "wasmer/static-web-server@=1.1.0"),
                ]),
                scripts: &["webserver"],
                env: Some(&[]),
                aliases: &[("static-web-server", "webserver")],
                architecture_dependencies: IndexMap::new(),
            },
        );
        map
    });
    &MAPPER
}

/// Python's `Path.absolute()`: prefix with the cwd, no normalization.
fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

fn path_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn prepare_command_name(index: usize) -> String {
    format!("{PREPARE_COMMAND_PREFIX}{index}")
}

fn command_requires_shell(command: &str) -> bool {
    command.chars().any(|character| {
        matches!(
            character,
            '|' | '&' | ';' | '<' | '>' | '$' | '*' | '{' | '}'
        )
    })
}

fn host_mount_arg(host_path: &Path, guest_path: &str) -> Result<String> {
    let host_path = absolute_path(host_path);
    std::fs::create_dir_all(&host_path)
        .with_context(|| format!("creating volume dir {}", host_path.display()))?;
    let resolved = host_path.canonicalize().unwrap_or(host_path);
    Ok(format!("--volume={}:{guest_path}", resolved.display()))
}

/// Port of `volumes.volume_mapdir_args`.
fn volume_mapdir_args(
    build_backend: &dyn BuildBackend,
    volume_mappings: &IndexMap<String, String>,
) -> Result<Vec<String>> {
    let mut args = Vec::new();
    for (name, guest_path) in volume_mappings {
        args.push(host_mount_arg(
            &build_backend.get_volume_path(name),
            guest_path,
        )?);
    }
    Ok(args)
}

/// Port of `WasmerRunner.main_args_for_module`.
pub fn main_args_for_module(module: &str, args: &[String]) -> Vec<String> {
    if !module.contains("edgejs") || args.iter().any(|arg| arg == "--bytecode-cache") {
        return args.to_vec();
    }
    let mut out = Vec::with_capacity(args.len() + 1);
    out.push("--bytecode-cache".to_owned());
    out.extend(args.iter().cloned());
    out
}

/// tomlkit's `array(items).multiline(True)`.
fn multiline_array(items: &[String]) -> Array {
    let mut arr = Array::new();
    for item in items {
        arr.push(item.as_str());
    }
    if !items.is_empty() {
        for item in arr.iter_mut() {
            item.decor_mut().set_prefix("\n    ");
            item.decor_mut().set_suffix("");
        }
        arr.set_trailing(",\n");
        arr.set_trailing_comma(false);
    }
    arr
}

/// Python `isinstance(config, PythonConfig)` — includes MkdocsConfig
/// (`MkdocsConfig(PythonConfig, StaticFileConfig)`).
fn is_python_config(provider: &str) -> bool {
    matches!(provider, "python" | "mkdocs")
}

/// Python `isinstance(config, PhpConfig)` — includes WordPress + Laravel.
fn is_php_config(provider: &str) -> bool {
    matches!(provider, "php" | "wordpress" | "laravel")
}

/// Python `isinstance(config, NodeConfig)` — includes node-static +
/// Laravel (`LaravelConfig(PhpConfig, NodeConfig)`).
fn is_node_config(provider: &str) -> bool {
    matches!(provider, "node" | "node-static" | "laravel")
}

/// Wasmer-specific overrides from `prepare_config`, applied through JSON to
/// keep the runner decoupled from sibling config-struct layouts.
fn apply_runner_flips(provider: &str, config_json: &mut JsonValue) {
    let Some(map) = config_json.as_object_mut() else {
        return;
    };
    if is_php_config(provider) {
        map.insert("phpix".to_owned(), JsonValue::Bool(true));
    }
    if is_node_config(provider) {
        map.insert("edgejs_enable".to_owned(), JsonValue::Bool(true));
        if map.get("edgejs_precompile").is_none_or(JsonValue::is_null) {
            // Nitro bundles can retain dependency sources that the recursive
            // EdgeJS precompiler cannot parse; loaded modules are still cached.
            let is_nitro = map.get("node_framework").and_then(JsonValue::as_str) == Some("nitro");
            map.insert("edgejs_precompile".to_owned(), JsonValue::Bool(!is_nitro));
        }
        map.insert(
            "node_remove_native_binaries".to_owned(),
            JsonValue::Bool(true),
        );
    }
}

pub(crate) fn provider_config_patch(config: &ProviderConfig) -> JsonValue {
    let provider = config.provider_name();
    let original = config.to_json();
    let mut effective = original.clone();
    if is_python_config(provider) {
        if let Some(map) = effective.as_object_mut() {
            map.insert(
                "python_extra_index_url".to_owned(),
                JsonValue::String("https://pythonindex.wasix.org/simple".to_owned()),
            );
            map.insert(
                "python_cross_platform".to_owned(),
                JsonValue::String("wasix_wasm32".to_owned()),
            );
            map.insert("python_precompile".to_owned(), JsonValue::Bool(true));
        }
    }
    apply_runner_flips(provider, &mut effective);

    let mut patch = serde_json::Map::new();
    if let (Some(original), Some(effective)) = (original.as_object(), effective.as_object()) {
        for (key, value) in effective {
            if original.get(key) != Some(value) {
                patch.insert(key.clone(), value.clone());
            }
        }
    }
    JsonValue::Object(patch)
}

use crate::build::report::{console_print, section_started};

#[cfg(test)]
#[derive(Debug, Clone)]
pub(crate) struct CapturedCommand {
    pub command: String,
    pub extra_args: Vec<String>,
    pub env: Option<IndexMap<String, String>>,
}

/// Port of `runners/wasmer.py::WasmerRunner`.
pub struct WasmerRunner {
    pub build_backend: Rc<RefCell<dyn BuildBackend>>,
    pub src_dir: PathBuf,
    pub anybuild_dir: PathBuf,
    pub wasmer_dir_path: PathBuf,
    pub wasmer_registry: Option<String>,
    pub wasmer_token: Option<String>,
    pub bin: String,
    /// Snapshot of the plan-visible config after Wasmer overrides. Feeds
    /// `anybuild.run/config` annotations.
    pub provider_config: Option<JsonValue>,
    operation: OperationContext,
    #[cfg(test)]
    pub(crate) captured_commands: Vec<CapturedCommand>,
}

impl WasmerRunner {
    pub fn new(
        build_backend: Rc<RefCell<dyn BuildBackend>>,
        src_dir: PathBuf,
        registry: Option<String>,
        token: Option<String>,
        bin: Option<String>,
        anybuild_dir: Option<PathBuf>,
        operation: OperationContext,
    ) -> Self {
        let anybuild_dir = anybuild_dir.unwrap_or_else(|| src_dir.join(".anybuild"));
        let wasmer_dir_path = anybuild_dir.join("wasmer");
        Self {
            build_backend,
            src_dir,
            anybuild_dir,
            wasmer_dir_path,
            wasmer_registry: registry,
            wasmer_token: token,
            bin: bin.unwrap_or_else(|| "wasmer".to_owned()),
            provider_config: None,
            operation,
            #[cfg(test)]
            captured_commands: Vec::new(),
        }
    }

    #[cfg(test)]
    fn prepare_config(&mut self, config: ProviderConfig) -> ProviderConfig {
        let config = config
            .merge_json(&provider_config_patch(&config))
            .unwrap_or(config);
        self.record_provider_config(&config);
        config
    }

    /// Query the Wasmer CLI version recorded alongside build artifacts.
    fn get_wasmer_version(&self) -> Result<String> {
        let mut command = std::process::Command::new(&self.bin);
        self.operation.prepare_command(&mut command);
        let output = command
            .arg("--version")
            .output()
            .with_context(|| format!("failed to run {} --version", self.bin))?;
        ensure!(
            output.status.success(),
            "{} --version failed with exit code {:?}",
            self.bin,
            output.status.code()
        );
        let stdout = String::from_utf8(output.stdout).context("Wasmer version is not UTF-8")?;
        parse_wasmer_version(&stdout)
    }

    fn write_build_annotations(&self) -> Result<()> {
        let version = self.get_wasmer_version()?;
        self.write_build_annotations_for_version(&version)
    }

    fn write_build_annotations_for_version(&self, version: &str) -> Result<()> {
        std::fs::create_dir_all(&self.wasmer_dir_path)?;
        let mut annotations = serde_yaml::Mapping::new();
        annotations.insert(yaml_str(WASMER_VERSION_ANNOTATION), yaml_str(version));
        let yaml = dump_yaml_sorted(&YamlValue::Mapping(annotations))?;
        std::fs::write(self.wasmer_dir_path.join(BUILD_ANNOTATIONS_FILENAME), yaml)?;
        Ok(())
    }

    fn load_build_annotations(&self) -> Result<serde_yaml::Mapping> {
        let path = self.wasmer_dir_path.join(BUILD_ANNOTATIONS_FILENAME);
        if !path.exists() {
            return Ok(serde_yaml::Mapping::new());
        }
        let text = std::fs::read_to_string(&path)?;
        match serde_yaml::from_str::<YamlValue>(&text)? {
            YamlValue::Mapping(annotations) => Ok(annotations),
            YamlValue::Null => Ok(serde_yaml::Mapping::new()),
            _ => bail!("{BUILD_ANNOTATIONS_FILENAME} must contain a dictionary"),
        }
    }

    /// Port of `build_prepare`.
    pub fn build_prepare(&self, serve: &Serve) -> Result<()> {
        let Some(prepare_steps) = serve.prepare.as_ref().filter(|steps| !steps.is_empty()) else {
            return Ok(());
        };
        let prepare_dir = self.wasmer_dir_path.join("prepare");
        std::fs::create_dir_all(&prepare_dir)?;

        let mut env: IndexMap<String, String> = serve.env.clone().unwrap_or_default();
        for dep in &serve.deps {
            if let Some(item) = mapper().get(dep.name.as_str()) {
                if let Some(dep_env) = item.env {
                    for (key, value) in dep_env {
                        env.insert((*key).to_owned(), (*value).to_owned());
                    }
                }
            }
        }
        let env_block = env
            .iter()
            .map(|(key, value)| format!("export {key}={value}"))
            .collect::<Vec<_>>()
            .join("\n");

        let mut commands: Vec<String> = Vec::new();
        if let Some(cwd) = serve.cwd.as_deref().filter(|cwd| !cwd.is_empty()) {
            commands.push(format!("cd {cwd}"));
        }
        for step in prepare_steps {
            commands.push(step.command.clone());
        }

        let body = std::iter::once(env_block)
            .chain(commands)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let content = format!("#!/bin/bash\n\n{body}");

        let script_path = prepare_dir.join("prepare.sh");
        std::fs::write(&script_path, &content)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))?;
        }
        Ok(())
    }

    /// Port of `find_file_in_mounts`.
    pub fn find_file_in_mounts(&self, serve: &Serve, program: &str) -> Option<PathBuf> {
        let program_path = Path::new(program);
        for mount in serve.mounts.as_deref().unwrap_or(&[]) {
            let serve_abs = absolute_path(&mount.serve_path);
            if program.starts_with(&path_str(&serve_abs)) {
                let module_path = program_path.strip_prefix(&mount.serve_path).ok()?;
                let full_module_path = self
                    .build_backend
                    .borrow()
                    .get_artifact_mount_path(&mount.name)
                    .join(module_path);
                return Some(full_module_path);
            }
        }
        None
    }

    /// Port of `build_serve`: generate wasmer.toml + app.yaml.
    pub fn build_serve(&self, serve: &Serve) -> Result<()> {
        let mut doc = DocumentMut::new();
        let mut package = Table::new();
        package.insert("entrypoint", value("start"));
        doc.insert("package", Item::Table(package));

        // binaries: script name -> (module, env)
        type BinaryEnv = Option<&'static [(&'static str, &'static str)]>;
        let mut binaries: IndexMap<String, (String, BinaryEnv)> = IndexMap::new();

        let mut deps: Vec<Package> = serve.deps.clone();
        if serve
            .prepare
            .as_ref()
            .is_some_and(|steps| !steps.is_empty())
            && !deps.iter().any(|dep| dep.name == "bash")
        {
            deps.push(Package {
                name: "bash".to_owned(),
                version: None,
                architecture: None,
            });
        }

        if !deps.is_empty() {
            section_started(&self.operation, "Wasmer Deployment");
        }
        let mut dependencies = Table::new();
        dependencies.decor_mut().set_prefix("\n");
        let mut dependency_names: Vec<String> = Vec::new();
        let mut package_mappings = Vec::new();
        for dep in &deps {
            let Some(item) = mapper().get(dep.name.as_str()) else {
                bail!("Dependency {} not found in Wasmer", dep.name);
            };
            let version = dep
                .version
                .clone()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "latest".to_owned());
            let mut mapped_dependencies = &item.dependencies;
            if let Some(architecture) = dep.architecture.as_deref().filter(|a| !a.is_empty()) {
                if let Some(architecture_dependencies) =
                    item.architecture_dependencies.get(architecture)
                {
                    if !architecture_dependencies.is_empty() {
                        mapped_dependencies = architecture_dependencies;
                    }
                }
            }
            let Some(mapped_dependency) = mapped_dependencies.get(version.as_str()) else {
                bail!("Dependency {}@{} not found in Wasmer", dep.name, version);
            };
            let source = dep
                .version
                .as_deref()
                .filter(|version| !version.is_empty())
                .map_or_else(
                    || dep.name.clone(),
                    |version| format!("{}@{version}", dep.name),
                );
            package_mappings.push(WasmerPackageMapping {
                source,
                target: (*mapped_dependency).to_owned(),
            });
            let (package_name, package_version) = mapped_dependency
                .split_once('@')
                .ok_or_else(|| anyhow!("invalid mapped dependency {mapped_dependency}"))?;
            dependencies.insert(package_name, value(package_version));
            dependency_names.push(package_name.to_owned());
            for script in item.scripts {
                binaries.insert(
                    (*script).to_owned(),
                    (format!("{package_name}:{script}"), item.env),
                );
            }
            for (alias, script) in item.aliases {
                binaries.insert(
                    (*alias).to_owned(),
                    (format!("{package_name}:{script}"), item.env),
                );
            }
        }
        if !package_mappings.is_empty() {
            self.operation.emit(Event::WasmerPackageMappings {
                mappings: package_mappings,
            });
        }
        doc.insert("dependencies", Item::Table(dependencies));

        let mut fs = Table::new();
        fs.decor_mut().set_prefix("\n");
        if let Some(mounts) = &serve.mounts {
            for mount in mounts {
                fs.insert(
                    &path_str(&absolute_path(&mount.serve_path)),
                    value(path_str(&absolute_path(
                        &self
                            .build_backend
                            .borrow()
                            .get_artifact_mount_path(&mount.name),
                    ))),
                );
            }
        }
        doc.insert("fs", Item::Table(fs));

        let mut modules: Option<ArrayOfTables> = None;

        let mut command_lines: Vec<(String, String)> = serve
            .commands
            .iter()
            .map(|(name, command)| (name.clone(), command.clone()))
            .collect();
        if let Some(prepare_steps) = &serve.prepare {
            for (index, step) in prepare_steps.iter().enumerate() {
                if command_requires_shell(&step.command) {
                    continue;
                }
                let Some(parts) = shlex::split(&step.command) else {
                    continue;
                };
                if parts
                    .first()
                    .is_some_and(|program| binaries.contains_key(program))
                {
                    command_lines.push((prepare_command_name(index), step.command.clone()));
                }
            }
        }

        if !command_lines.is_empty() {
            let mut commands = ArrayOfTables::new();
            for (index, (command_name, command_line)) in command_lines.iter().enumerate() {
                let mut parts = shlex::split(command_line)
                    .ok_or_else(|| anyhow!("Could not parse command: {command_line}"))?;
                ensure!(!parts.is_empty(), "Command {command_name} is empty");
                let mut program = parts[0].clone();
                let mut command_env: IndexMap<String, String> = IndexMap::new();
                let mut command_module: Option<String> = None;
                if let Some(rewritten) = rewrite_binary(&program) {
                    let rewritten_program = shlex::split(rewritten)
                        .ok_or_else(|| anyhow!("Could not parse rewrite: {rewritten}"))?;
                    program = rewritten_program[0].clone();
                    parts.splice(0..1, rewritten_program);
                } else if !binaries.contains_key(&program) {
                    if program.starts_with('/') {
                        // It's an executable
                        let full_module_path =
                            self.find_file_in_mounts(serve, &program).ok_or_else(|| {
                                anyhow!(
                                    "Could not find {program} in the mounts or in the binaries list"
                                )
                            })?;
                        // Check if is a WebAssembly module by checking the contents
                        let contents = std::fs::read(&full_module_path)
                            .with_context(|| format!("reading {}", full_module_path.display()))?;
                        ensure!(
                            contents.starts_with(b"\0asm"),
                            "Wasmer can only execute WebAssembly modules, binary {program} is not."
                        );

                        let modules = modules.get_or_insert_with(ArrayOfTables::new);
                        let mut module = Table::new();
                        module
                            .decor_mut()
                            .set_prefix(if modules.is_empty() { "" } else { "\n" });
                        let module_name = program
                            .replace('/', "_")
                            .to_lowercase()
                            .trim_start_matches('_')
                            .to_owned();
                        module.insert("name", value(&module_name));
                        module.insert("source", value(path_str(&absolute_path(&full_module_path))));
                        modules.push(module);

                        command_module = Some(module_name);
                    } else {
                        bail!("Binary {program} not runable in Wasmer yet");
                    }
                }

                if command_module.is_none() {
                    let Some((module, program_env)) = binaries.get(&program) else {
                        bail!(
                            "Command {} is trying to run {} but it is not available (dependencies: {}, programs: {})",
                            command_name,
                            program,
                            dependency_names.join(", "),
                            binaries.keys().cloned().collect::<Vec<_>>().join(", ")
                        );
                    };
                    command_module = Some(module.clone());
                    for (key, value) in program_env.unwrap_or(&[]) {
                        command_env.insert((*key).to_owned(), (*value).to_owned());
                    }
                }
                let command_module = command_module.expect("command module resolved");

                let mut command = Table::new();
                // tomlkit: no blank line before the first [[command]],
                // one between subsequent elements.
                command
                    .decor_mut()
                    .set_prefix(if index > 0 { "\n" } else { "" });
                command.insert("name", value(command_name));
                command.insert("module", value(&command_module));
                command.insert("runner", value("wasi"));
                let mut wasi_args = Table::new();
                wasi_args.decor_mut().set_prefix("\n");
                if let Some(cwd) = serve.cwd.as_deref().filter(|cwd| !cwd.is_empty()) {
                    wasi_args.insert("cwd", value(cwd));
                }
                let main_args = if command_name.starts_with(PREPARE_COMMAND_PREFIX) {
                    parts[1..].to_vec()
                } else {
                    main_args_for_module(&command_module, &parts[1..])
                };
                wasi_args.insert("main-args", value(multiline_array(&main_args)));
                if let Some(serve_env) = &serve.env {
                    for (key, value) in serve_env {
                        command_env.insert(key.clone(), value.clone());
                    }
                }
                if !command_env.is_empty() {
                    let env_items: Vec<String> = command_env
                        .iter()
                        .map(|(key, value)| format!("{key}={value}"))
                        .collect();
                    wasi_args.insert("env", value(multiline_array(&env_items)));
                }
                command.insert("annotations.wasi", Item::Table(wasi_args));
                commands.push(command);
            }
            doc.insert("command", Item::ArrayOfTables(commands));
            if let Some(modules) = modules {
                doc.insert("module", Item::ArrayOfTables(modules));
            }
        }

        std::fs::create_dir_all(&self.wasmer_dir_path)?;

        let manifest = format!(
            "# Wasmer manifest generated with Anybuild v{}\n\n{}",
            anybuild_version(),
            doc
        )
        .replace(
            "[command.\"annotations.wasi\"]",
            "[command.annotations.wasi]",
        );
        self.operation.emit(Event::WasmerFileContent {
            filename: "wasmer.toml".to_owned(),
            content: manifest.trim().to_owned(),
            language: "toml".to_owned(),
        });
        std::fs::write(self.wasmer_dir_path.join("wasmer.toml"), &manifest)?;

        // ---- app.yaml ----
        let original_app_yaml_path = self.src_dir.join("app.yaml");
        let mut yaml_config: serde_yaml::Mapping = if original_app_yaml_path.exists() {
            console_print(
                &self.operation,
                "Using original app.yaml found in source directory",
            );
            let text = std::fs::read_to_string(&original_app_yaml_path)?;
            match serde_yaml::from_str::<YamlValue>(&text)? {
                YamlValue::Mapping(map) => map,
                YamlValue::Null => serde_yaml::Mapping::new(),
                _ => bail!("app.yaml must be a mapping"),
            }
        } else {
            let mut map = serde_yaml::Mapping::new();
            map.insert(yaml_str("kind"), yaml_str("wasmer.io/App.v0"));
            map
        };
        yaml_config.insert(yaml_str("package"), yaml_str("."));

        if let Some(services) = serve.services.as_ref().filter(|s| !s.is_empty()) {
            let mut capabilities = take_mapping(
                &mut yaml_config,
                "capabilities",
                "capabilities must be a dictionary",
            )?;
            let has_mysql = services.iter().any(|service| service.provider == "mysql");
            if has_mysql {
                let mut database = serde_yaml::Mapping::new();
                database.insert(yaml_str("engine"), yaml_str("mysql"));
                capabilities.insert(yaml_str("database"), YamlValue::Mapping(database));
            }
            yaml_config.insert(yaml_str("capabilities"), YamlValue::Mapping(capabilities));
        }

        if let Some(volumes) = serve.volumes.as_ref().filter(|v| !v.is_empty()) {
            let mut volumes_yaml: Vec<YamlValue> = match yaml_config.remove(yaml_str("volumes")) {
                Some(YamlValue::Sequence(seq)) => seq,
                Some(_) => bail!("volumes must be a list"),
                None => Vec::new(),
            };
            for vol in volumes {
                let mount_path = path_str(&vol.serve_path);
                let existing_volume = volumes_yaml.iter_mut().find(|volume_yaml| {
                    volume_yaml.as_mapping().is_some_and(|map| {
                        map.get(yaml_str("mount")) == Some(&yaml_str(&mount_path))
                    })
                });
                if let Some(existing_volume) = existing_volume {
                    existing_volume
                        .as_mapping_mut()
                        .expect("checked mapping")
                        .insert(yaml_str("name"), yaml_str(&vol.name));
                } else {
                    let mut volume = serde_yaml::Mapping::new();
                    volume.insert(yaml_str("name"), yaml_str(&vol.name));
                    volume.insert(yaml_str("mount"), yaml_str(&mount_path));
                    volumes_yaml.push(YamlValue::Mapping(volume));
                }
            }
            yaml_config.insert(yaml_str("volumes"), YamlValue::Sequence(volumes_yaml));
        }

        let has_php = serve.deps.iter().any(|dep| dep.name == "php");
        let has_phpix = serve.deps.iter().any(|dep| dep.name == "phpix");
        let build_deps: Vec<&Package> = serve
            .build
            .iter()
            .filter_map(|step| match step {
                Step::Use(UseStep { dependencies }) => Some(dependencies.iter()),
                _ => None,
            })
            .flatten()
            .collect();
        let is_built_with_go = build_deps
            .iter()
            .any(|dep| dep.name == "go" || dep.name == "go-wasix");

        // Note: phpix is multi-threaded, so this is only needed for raw php server and go.
        if (has_php && !has_phpix) || is_built_with_go {
            let mut scaling =
                take_mapping(&mut yaml_config, "scaling", "scaling must be a dictionary")?;
            scaling.insert(yaml_str("mode"), yaml_str("single_concurrency"));
            yaml_config.insert(yaml_str("scaling"), YamlValue::Mapping(scaling));
        }

        if has_phpix && serve.provider == "wordpress" {
            let mut capabilities = take_mapping(
                &mut yaml_config,
                "capabilities",
                "capabilities must be a dictionary",
            )?;
            let mut memory = match capabilities.remove(yaml_str("memory")) {
                Some(YamlValue::Mapping(map)) => map,
                Some(_) => bail!("memory must be a dictionary"),
                None => serde_yaml::Mapping::new(),
            };
            memory.insert(yaml_str("limit"), yaml_str("2Gb"));
            capabilities.insert(yaml_str("memory"), YamlValue::Mapping(memory));
            yaml_config.insert(yaml_str("capabilities"), YamlValue::Mapping(capabilities));
            yaml_config.insert(yaml_str("enable_email"), YamlValue::Bool(true));
        }

        if has_phpix {
            if let Some(threads) = serve
                .env
                .as_ref()
                .and_then(|env| env.get("PHPIX_PHP_THREADS"))
                .filter(|threads| !threads.is_empty())
            {
                let mut app_env =
                    take_mapping(&mut yaml_config, "env", "env must be a dictionary")?;
                app_env.insert(yaml_str("PHPIX_PHP_THREADS"), yaml_str(threads));
                yaml_config.insert(yaml_str("env"), YamlValue::Mapping(app_env));
            }
        }

        if serve.commands.contains_key("after_deploy") {
            let mut jobs: Vec<YamlValue> = match yaml_config.remove(yaml_str("jobs")) {
                Some(YamlValue::Sequence(seq)) => seq,
                _ => Vec::new(),
            };
            // Filter out any existing after_deploy job
            jobs.retain(|job| {
                job.as_mapping().and_then(|map| map.get(yaml_str("name")))
                    != Some(&yaml_str("after_deploy"))
            });
            let mut execute = serde_yaml::Mapping::new();
            execute.insert(yaml_str("command"), yaml_str("after_deploy"));
            let mut action = serde_yaml::Mapping::new();
            action.insert(yaml_str("execute"), YamlValue::Mapping(execute));
            let mut job = serde_yaml::Mapping::new();
            job.insert(yaml_str("name"), yaml_str("after_deploy"));
            job.insert(yaml_str("trigger"), yaml_str("post-deployment"));
            job.insert(yaml_str("action"), YamlValue::Mapping(action));
            jobs.push(YamlValue::Mapping(job));
            yaml_config.insert(yaml_str("jobs"), YamlValue::Sequence(jobs));
        }

        let mut annotations = take_mapping(
            &mut yaml_config,
            "annotations",
            "annotations must be a dictionary",
        )?;
        for (key, value) in self.load_build_annotations()? {
            annotations.insert(key, value);
        }
        let config_annotation =
            serialize_provider_config(&serve.provider, self.provider_config.as_ref());
        annotations.insert(
            yaml_str(ANYBUILD_CONFIG_ANNOTATION),
            serde_yaml::to_value(&config_annotation)?,
        );
        annotations.insert(
            yaml_str(ANYBUILD_VERSION_ANNOTATION),
            yaml_str(anybuild_version()),
        );
        annotations.insert(
            yaml_str(ANYBUILD_PROVIDER_ANNOTATION),
            yaml_str(&serve.provider),
        );
        let framework = self
            .provider_config
            .as_ref()
            .and_then(|config| {
                config
                    .get("node_framework")
                    .or_else(|| config.get("python_framework"))
                    .or_else(|| config.get("php_framework"))
            })
            .and_then(JsonValue::as_str);
        if let Some(app_kind) = resolve_app_kind(&serve.provider, framework) {
            annotations.insert(yaml_str(WASMER_APP_KIND_ANNOTATION), yaml_str(&app_kind));
        }
        yaml_config.insert(yaml_str("annotations"), YamlValue::Mapping(annotations));

        let app_yaml = dump_yaml_sorted(&YamlValue::Mapping(yaml_config))?;

        self.operation.emit(Event::WasmerFileContent {
            filename: "app.yaml".to_owned(),
            content: app_yaml.trim().to_owned(),
            language: "yaml".to_owned(),
        });
        std::fs::write(self.wasmer_dir_path.join("app.yaml"), &app_yaml)?;
        Ok(())
    }

    /// Port of `command_uses_edgejs`. (The `--experimental-napi` flag it
    /// used to gate is currently commented out, as in Python.)
    #[cfg(test)]
    fn command_uses_edgejs(&self, command: &str) -> bool {
        let wasmer_toml_path = self.wasmer_dir_path.join("wasmer.toml");
        let Ok(text) = std::fs::read_to_string(&wasmer_toml_path) else {
            return false;
        };
        let Ok(manifest) = text.parse::<DocumentMut>() else {
            return false;
        };
        let Some(commands) = manifest.get("command").and_then(Item::as_array_of_tables) else {
            return false;
        };
        for item in commands.iter() {
            if item.get("name").and_then(Item::as_str) != Some(command) {
                continue;
            }
            let module = item.get("module").and_then(Item::as_str).unwrap_or("");
            let atom = item
                .get("annotations")
                .and_then(Item::as_table_like)
                .and_then(|annotations| annotations.get("wasi"))
                .and_then(Item::as_table_like)
                .and_then(|wasi| wasi.get("atom"))
                .and_then(Item::as_str);
            return module.contains("edgejs") || atom == Some("edgejs");
        }
        false
    }

    /// Port of `run_command`: `subprocess.run(check=True)`.
    pub fn run_command(
        &mut self,
        command: &str,
        extra_args: &[String],
        env: Option<&IndexMap<String, String>>,
    ) -> Result<()> {
        #[cfg(test)]
        {
            self.captured_commands.push(CapturedCommand {
                command: command.to_owned(),
                extra_args: extra_args.to_vec(),
                env: env.cloned(),
            });
            Ok(())
        }
        #[cfg(not(test))]
        {
            let mut cmd = std::process::Command::new(command);
            self.operation.prepare_command(&mut cmd);
            cmd.args(extra_args);
            if let Some(env) = env {
                cmd.env_clear();
                cmd.envs(env);
            }
            let status = self
                .operation
                .command_status(&mut cmd)
                .with_context(|| format!("failed to run {command}"))?;
            ensure!(
                status.success(),
                "Command {command} failed with exit code {:?}",
                status.code()
            );
            Ok(())
        }
    }

    /// Port of `_update_app_yaml`.
    fn update_app_yaml(&self, app_owner: Option<&str>, app_name: Option<&str>) -> Result<()> {
        let app_owner = app_owner.filter(|owner| !owner.is_empty());
        let app_name = app_name.filter(|name| !name.is_empty());
        if app_owner.is_none() && app_name.is_none() {
            return Ok(());
        }
        let app_yaml_path = self.wasmer_dir_path.join("app.yaml");
        if !app_yaml_path.exists() {
            return Ok(());
        }
        let text = std::fs::read_to_string(&app_yaml_path)?;
        let mut yaml_config = match serde_yaml::from_str::<YamlValue>(&text)? {
            YamlValue::Mapping(map) => map,
            YamlValue::Null => serde_yaml::Mapping::new(),
            _ => bail!("app.yaml must be a mapping"),
        };
        let mut changed = false;
        if let Some(owner) = app_owner {
            if yaml_config.get(yaml_str("owner")) != Some(&yaml_str(owner)) {
                yaml_config.insert(yaml_str("owner"), yaml_str(owner));
                changed = true;
            }
        }
        if let Some(name) = app_name {
            if yaml_config.get(yaml_str("name")) != Some(&yaml_str(name)) {
                yaml_config.insert(yaml_str("name"), yaml_str(name));
                changed = true;
            }
        }
        if changed {
            std::fs::write(
                &app_yaml_path,
                dump_yaml_sorted(&YamlValue::Mapping(yaml_config))?,
            )?;
        }
        Ok(())
    }

    /// Port of `_deploy_args`.
    fn deploy_args(&self, app_owner: Option<&str>, app_name: Option<&str>) -> Vec<String> {
        let mut extra_args: Vec<String> = Vec::new();
        if let Some(registry) = &self.wasmer_registry {
            extra_args.extend(["--registry".to_owned(), registry.clone()]);
        }
        if let Some(token) = &self.wasmer_token {
            extra_args.extend(["--token".to_owned(), token.clone()]);
        }
        if let Some(owner) = app_owner.filter(|owner| !owner.is_empty()) {
            extra_args.extend(["--owner".to_owned(), owner.to_owned()]);
        }
        if let Some(name) = app_name.filter(|name| !name.is_empty()) {
            extra_args.extend(["--app-name".to_owned(), name.to_owned()]);
        }
        let mut args = vec![
            "deploy".to_owned(),
            "--publish-package".to_owned(),
            "--dir".to_owned(),
            path_str(&self.wasmer_dir_path),
            "--non-interactive".to_owned(),
        ];
        args.extend(extra_args);
        args
    }

    /// Port of `deploy_config`: build package.webc and write the deploy
    /// descriptor JSON (byte-compatible with Python's `json.dumps`).
    pub fn deploy_config(&mut self, config_path: &Path) -> Result<()> {
        let package_webc_path = self.wasmer_dir_path.join("package.webc");
        let app_yaml_path = self.wasmer_dir_path.join("app.yaml");
        if let Some(parent) = package_webc_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bin = self.bin.clone();
        self.run_command(
            &bin,
            &[
                "package".to_owned(),
                "build".to_owned(),
                path_str(&self.wasmer_dir_path),
                "--out".to_owned(),
                path_str(&package_webc_path),
            ],
            None,
        )?;
        let contents = std::fs::read(&package_webc_path)
            .with_context(|| format!("reading {}", package_webc_path.display()))?;
        let sha256 = format!("{:x}", sha2::Sha256::digest(&contents));
        // json.dumps default separators: (", ", ": ").
        let payload = format!(
            "{{\"app_yaml_path\": {}, \"package_webc_path\": {}, \"package_webc_size\": {}, \"package_webc_sha256\": {}}}",
            serde_json::to_string(&path_str(&absolute_path(&app_yaml_path)))?,
            serde_json::to_string(&path_str(&absolute_path(&package_webc_path)))?,
            contents.len(),
            serde_json::to_string(&sha256)?,
        );
        std::fs::write(config_path, payload)?;
        console_print(
            &self.operation,
            &format!("\nSaved deploy config to {}", config_path.display()),
        );
        Ok(())
    }

    /// Port of `deploy`.
    pub fn deploy(&mut self, app_owner: Option<&str>, app_name: Option<&str>) -> Result<()> {
        self.update_app_yaml(app_owner, app_name)?;
        let bin = self.bin.clone();
        let args = self.deploy_args(app_owner, app_name);
        self.run_command(&bin, &args, None)
    }
}

impl Runner for WasmerRunner {
    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn record_provider_config(&mut self, config: &ProviderConfig) {
        self.provider_config = Some(config.to_json());
    }

    /// Port of `prepare_build_steps`: swap go for go-wasix and inject the
    /// GOOS/GOARCH env step.
    fn prepare_build_steps(&self, steps: Vec<Step>) -> Vec<Step> {
        let mut new_build_steps: Vec<Step> = Vec::new();
        for step in steps {
            if let Step::Use(use_step) = &step {
                let mut deps: Vec<Package> = Vec::new();
                let mut found_go = false;
                for package in &use_step.dependencies {
                    if package.name == "go" {
                        deps.push(Package {
                            name: "go-wasix".to_owned(),
                            version: None,
                            architecture: None,
                        });
                        found_go = true;
                    } else {
                        deps.push(package.clone());
                    }
                }
                if found_go {
                    new_build_steps.push(Step::Use(UseStep { dependencies: deps }));
                    let mut variables: IndexMap<String, String> = IndexMap::new();
                    variables.insert("GOOS".to_owned(), "wasip1".to_owned());
                    variables.insert("GOARCH".to_owned(), "wasm".to_owned());
                    new_build_steps.push(Step::Env(EnvStep { variables }));
                } else {
                    new_build_steps.push(step);
                }
            } else {
                new_build_steps.push(step);
            }
        }
        new_build_steps
    }

    fn build(&mut self, serve: &Serve) -> Result<()> {
        self.write_build_annotations()?;
        self.build_prepare(serve)?;
        self.build_serve(serve)
    }

    /// Run dependency-backed preparation directly, falling back to the
    /// generated shell script for commands that need shell syntax.
    fn prepare(&mut self, _env: &IndexMap<String, String>, prepare: &[RunStep]) -> Result<()> {
        let direct_commands: Vec<String> = (0..prepare.len()).map(prepare_command_name).collect();
        if !direct_commands.is_empty()
            && direct_commands
                .iter()
                .all(|command| self.has_serve_command(command))
        {
            let volume_mappings = load_volume_mappings(&self.anybuild_dir)?;
            for command in direct_commands {
                self.run_serve_command(&command, Some(&volume_mappings), &[], None)?;
            }
            return Ok(());
        }

        let prepare_dir = self.wasmer_dir_path.join("prepare");
        let volume_mappings = load_volume_mappings(&self.anybuild_dir)?;
        self.run_serve_command(
            "bash /prepare/prepare.sh",
            Some(&volume_mappings),
            &[HostMount {
                host_path: &prepare_dir,
                guest_path: "/prepare",
            }],
            None,
        )
    }

    /// Port of `has_serve_command`.
    fn has_serve_command(&self, command: &str) -> bool {
        let wasmer_toml_path = self.wasmer_dir_path.join("wasmer.toml");
        let Ok(text) = std::fs::read_to_string(&wasmer_toml_path) else {
            return false;
        };
        let Ok(manifest) = text.parse::<DocumentMut>() else {
            return false;
        };
        let Some(commands) = manifest.get("command").and_then(Item::as_array_of_tables) else {
            return false;
        };
        let found = commands
            .iter()
            .any(|item| item.get("name").and_then(Item::as_str) == Some(command));
        found
    }

    /// Port of `run_serve_command`: spawn `wasmer run` with mapped dirs
    /// and the merged host environment.
    fn run_serve_command(
        &mut self,
        command: &str,
        volume_mappings: Option<&IndexMap<String, String>>,
        host_mounts: &[HostMount<'_>],
        env: Option<&IndexMap<String, String>>,
    ) -> Result<()> {
        let parsed_command = shlex::split(command).unwrap_or_default();
        if parsed_command.is_empty() {
            bail!("Serve command cannot be empty");
        }
        let command_name = parsed_command[0].clone();
        let command_args = &parsed_command[1..];
        let mut extra_args: Vec<String> = Vec::new();

        // if self.command_uses_edgejs(&command_name) {
        //     run_args.push("--experimental-napi");
        // }

        if let Some(registry) = &self.wasmer_registry {
            extra_args.insert(0, format!("--registry={registry}"));
        }
        let mut run_env = self.operation.process_environment();
        if let Some(env) = env {
            for (key, value) in env {
                run_env.insert(key.clone(), value.clone());
            }
        }
        let empty = IndexMap::new();
        let mut args: Vec<String> = vec!["run".to_owned()];
        args.push(path_str(&absolute_path(&self.wasmer_dir_path)));
        args.push("--net".to_owned());
        args.push("--forward-host-env".to_owned());
        args.push(format!("--command={command_name}"));
        args.extend(volume_mapdir_args(
            &*self.build_backend.borrow(),
            volume_mappings.unwrap_or(&empty),
        )?);
        for mount in host_mounts {
            args.push(host_mount_arg(mount.host_path, mount.guest_path)?);
        }
        args.extend(extra_args);
        if !command_args.is_empty() {
            args.push("--".to_owned());
            args.extend(command_args.iter().cloned());
        }
        let bin = self.bin.clone();
        self.run_command(&bin, &args, Some(&run_env))
    }
}

fn yaml_str(value: &str) -> YamlValue {
    YamlValue::String(value.to_owned())
}

fn parse_wasmer_version(output: &str) -> Result<String> {
    let output = output.trim();
    let Some((name, version)) = output.split_once(' ') else {
        bail!("Unexpected Wasmer version output: {output:?}");
    };
    ensure!(
        name.eq_ignore_ascii_case("wasmer") && !version.is_empty(),
        "Unexpected Wasmer version output: {output:?}"
    );
    Ok(version.to_owned())
}

/// `yaml_config.get(key, {})` + isinstance assert, removing the key so the
/// caller can re-insert the updated mapping (Python mutates in place).
fn take_mapping(
    yaml_config: &mut serde_yaml::Mapping,
    key: &str,
    message: &'static str,
) -> Result<serde_yaml::Mapping> {
    match yaml_config.remove(yaml_str(key)) {
        Some(YamlValue::Mapping(map)) => Ok(map),
        Some(_) => bail!("{message}"),
        None => Ok(serde_yaml::Mapping::new()),
    }
}

/// PyYAML's `yaml.dump`: block style with keys sorted at every level.
fn dump_yaml_sorted(value: &YamlValue) -> Result<String> {
    fn sort(value: &YamlValue) -> YamlValue {
        match value {
            YamlValue::Mapping(map) => {
                let mut entries: Vec<(YamlValue, YamlValue)> =
                    map.iter().map(|(k, v)| (k.clone(), sort(v))).collect();
                entries.sort_by_key(|a| yaml_key(&a.0));
                YamlValue::Mapping(entries.into_iter().collect())
            }
            YamlValue::Sequence(items) => YamlValue::Sequence(items.iter().map(sort).collect()),
            other => other.clone(),
        }
    }
    fn yaml_key(value: &YamlValue) -> String {
        match value {
            YamlValue::String(s) => s.clone(),
            other => serde_yaml::to_string(other).unwrap_or_default(),
        }
    }
    Ok(serde_yaml::to_string(&sort(value))?)
}

#[cfg(test)]
mod tests {
    //! Port of `tests/test_wasmer_annotations.py`.

    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::event::{ProcessIo, Reporter};
    use crate::plan::{Mount, Volume};
    use crate::providers::node::NodeConfig;
    use crate::providers::python::{PythonConfig, PythonFramework};

    struct DummyBuildBackend {
        root: PathBuf,
    }

    impl BuildBackend for DummyBuildBackend {
        fn build(
            &mut self,
            _name: &str,
            _env: &IndexMap<String, String>,
            _mounts: &[Mount],
            _steps: &[Step],
        ) -> Result<()> {
            unimplemented!()
        }
        fn get_build_mount_path(&self, name: &str) -> PathBuf {
            self.root.join("build").join(name)
        }
        fn get_artifact_mount_path(&self, name: &str) -> PathBuf {
            let path = self.root.join("artifacts").join(name);
            std::fs::create_dir_all(&path).unwrap();
            path
        }
        fn get_volume_path(&self, name: &str) -> PathBuf {
            self.root.join("volumes").join(name)
        }
        fn get_runtime_path(&self) -> Option<String> {
            None
        }
    }

    fn make_runner(root: &Path) -> WasmerRunner {
        let src_dir = root.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        WasmerRunner::new(
            Rc::new(RefCell::new(DummyBuildBackend {
                root: root.to_path_buf(),
            })),
            src_dir,
            None,
            None,
            Some("wasmer".to_owned()),
            None,
            OperationContext::for_test(),
        )
    }

    fn make_runner_with_events(root: &Path) -> (WasmerRunner, Arc<Mutex<Vec<Event>>>) {
        let src_dir = root.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        let operation = OperationContext::new(
            IndexMap::new(),
            false,
            ProcessIo::Inherit,
            Reporter::new(move |event: &Event| {
                captured.lock().unwrap().push(event.clone());
            }),
        );
        let runner = WasmerRunner::new(
            Rc::new(RefCell::new(DummyBuildBackend {
                root: root.to_path_buf(),
            })),
            src_dir,
            None,
            None,
            Some("wasmer".to_owned()),
            None,
            operation,
        );
        (runner, events)
    }

    fn package(name: &str, version: Option<&str>, architecture: Option<&str>) -> Package {
        Package {
            name: name.to_owned(),
            version: version.map(str::to_owned),
            architecture: architecture.map(str::to_owned),
        }
    }

    fn serve(
        name: &str,
        provider: &str,
        deps: Vec<Package>,
        cwd: Option<&str>,
        commands: &[(&str, &str)],
    ) -> Serve {
        Serve {
            name: name.to_owned(),
            provider: provider.to_owned(),
            build: vec![],
            deps,
            commands: commands
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            cwd: cwd.map(str::to_owned),
            prepare: None,
            mounts: None,
            volumes: None,
            env: None,
            services: None,
        }
    }

    fn read_yaml(path: &Path) -> serde_yaml::Mapping {
        let text = std::fs::read_to_string(path).unwrap();
        match serde_yaml::from_str::<YamlValue>(&text).unwrap() {
            YamlValue::Mapping(map) => map,
            other => panic!("expected mapping, got {other:?}"),
        }
    }

    fn read_toml(path: &Path) -> DocumentMut {
        std::fs::read_to_string(path)
            .unwrap()
            .parse::<DocumentMut>()
            .unwrap()
    }

    #[test]
    fn test_resolve_app_kind() {
        let cases: &[(&str, Option<&str>, Option<&str>)] = &[
            ("wordpress", None, Some("wordpress")),
            ("python", Some("django"), Some("django")),
            ("python", Some("mcp"), Some("mcp")),
            ("php", Some("moodle"), Some("moodle")),
            ("php", Some("drupal"), Some("drupal")),
            ("javascript", Some("ghost"), Some("ghost")),
            ("javascript", Some("strapi"), Some("strapi")),
            ("node", Some("ghost"), Some("ghost")),
            ("node-static", Some("ghost"), Some("ghost")),
            ("python", Some("fastapi"), None),
        ];
        for (provider, framework, expected) in cases {
            assert_eq!(
                resolve_app_kind(provider, *framework).as_deref(),
                *expected,
                "provider={provider} framework={framework:?}"
            );
        }
    }

    #[test]
    fn test_anybuild_annotations_use_public_domain() {
        assert_eq!(ANYBUILD_CONFIG_ANNOTATION, "anybuild.run/config");
        assert_eq!(ANYBUILD_PROVIDER_ANNOTATION, "anybuild.run/provider");
        assert_eq!(ANYBUILD_VERSION_ANNOTATION, "anybuild.run/version");
    }

    #[test]
    fn test_wasmer_deployment_events_are_emitted_in_order() {
        let tmp = tempfile::tempdir().unwrap();
        let (runner, events) = make_runner_with_events(tmp.path());
        let mut serve = serve(
            "static",
            "staticfile",
            vec![package("static-web-server", None, None)],
            None,
            &[("start", "static-web-server")],
        );
        serve.prepare = Some(vec![RunStep {
            command: "true".to_owned(),
            inputs: None,
            outputs: None,
            group: None,
        }]);

        runner.build_prepare(&serve).unwrap();
        runner.build_serve(&serve).unwrap();

        let events = events.lock().unwrap();
        let deployment = events
            .iter()
            .position(
                |event| matches!(event, Event::SectionStarted { title } if title == "Wasmer Deployment"),
            )
            .unwrap();
        let mapping = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    Event::WasmerPackageMappings { mappings }
                        if mappings == &[
                            WasmerPackageMapping {
                                source: "static-web-server".to_owned(),
                                target: "wasmer/static-web-server@=1.1.0".to_owned(),
                            },
                            WasmerPackageMapping {
                                source: "bash".to_owned(),
                                target: "wasmer/bash@=1.0.25".to_owned(),
                            },
                        ]
                )
            })
            .unwrap();
        let wasmer_content = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    Event::WasmerFileContent { filename, .. }
                        if filename == "wasmer.toml"
                )
            })
            .unwrap();
        let app_content = events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    Event::WasmerFileContent { filename, .. }
                        if filename == "app.yaml"
                )
            })
            .unwrap();

        assert!(deployment < mapping);
        assert!(mapping < wasmer_content);
        assert!(wasmer_content < app_content);
    }

    #[test]
    fn test_wasmer_app_yaml_adds_python_annotations() {
        let tmp = tempfile::tempdir().unwrap();
        let mut runner = make_runner(tmp.path());
        std::fs::write(
            runner.src_dir.join("app.yaml"),
            "annotations:\n  example.com/existing: keep\n",
        )
        .unwrap();

        let config = PythonConfig {
            framework: Some(PythonFramework::Django),
            ..PythonConfig::default()
        };
        runner.prepare_config(ProviderConfig::Python(config));

        let serve = serve(
            "django",
            "python",
            vec![package("python", None, None)],
            None,
            &[(
                "start",
                "python -m uvicorn example.asgi --host 0.0.0.0 --port 8080",
            )],
        );

        runner.write_build_annotations_for_version("7.2.0").unwrap();
        runner.build_serve(&serve).unwrap();

        let app_yaml = read_yaml(&runner.wasmer_dir_path.join("app.yaml"));
        let build_annotations = read_yaml(&runner.wasmer_dir_path.join(BUILD_ANNOTATIONS_FILENAME));
        let annotations = app_yaml
            .get(yaml_str("annotations"))
            .and_then(YamlValue::as_mapping)
            .unwrap();

        assert_eq!(
            annotations.get(yaml_str("example.com/existing")),
            Some(&yaml_str("keep"))
        );
        assert_eq!(
            annotations.get(yaml_str(ANYBUILD_PROVIDER_ANNOTATION)),
            Some(&yaml_str("python"))
        );
        assert_eq!(
            annotations.get(yaml_str(ANYBUILD_VERSION_ANNOTATION)),
            Some(&yaml_str(anybuild_version()))
        );
        assert_eq!(
            annotations.get(yaml_str(WASMER_APP_KIND_ANNOTATION)),
            Some(&yaml_str("django"))
        );
        assert_eq!(
            build_annotations.get(yaml_str(WASMER_VERSION_ANNOTATION)),
            Some(&yaml_str("7.2.0"))
        );
        assert_eq!(
            annotations.get(yaml_str(WASMER_VERSION_ANNOTATION)),
            Some(&yaml_str("7.2.0"))
        );
        let config = annotations
            .get(yaml_str(ANYBUILD_CONFIG_ANNOTATION))
            .and_then(YamlValue::as_mapping)
            .unwrap();
        assert_eq!(
            config.get(yaml_str("python_framework")),
            Some(&yaml_str("django"))
        );
        assert_eq!(
            config.get(yaml_str("python_cross_platform")),
            Some(&yaml_str("wasix_wasm32"))
        );
        assert_eq!(
            config.get(yaml_str("python_extra_index_url")),
            Some(&yaml_str("https://pythonindex.wasix.org/simple"))
        );
        assert!(!config.contains_key(yaml_str("python_version")));
        assert!(!config.contains_key(yaml_str("python_precompile")));
    }

    #[test]
    fn test_parse_wasmer_version() {
        assert_eq!(parse_wasmer_version("wasmer 7.2.0\n").unwrap(), "7.2.0");
        assert_eq!(parse_wasmer_version("Wasmer 8.0.0").unwrap(), "8.0.0");
        assert!(parse_wasmer_version("7.2.0").is_err());
        assert!(parse_wasmer_version("other 7.2.0").is_err());
    }

    #[test]
    fn test_config_annotation_matches_python_byte_for_byte() {
        // Golden captured from the compatibility fixture (PythonConfig with
        // python_framework=django through prepare_config + serialize), dumped as
        // sorted JSON. Pins exclude_none semantics and enum/default values.
        let tmp = tempfile::tempdir().unwrap();
        let mut runner = make_runner(tmp.path());
        runner.prepare_config(ProviderConfig::Python(PythonConfig {
            framework: Some(PythonFramework::Django),
            ..PythonConfig::default()
        }));
        let annotation = serialize_provider_config("python", runner.provider_config.as_ref());
        let mut sorted = std::collections::BTreeMap::new();
        for (key, value) in annotation.as_object().unwrap() {
            sorted.insert(key.clone(), value.clone());
        }
        let json = serde_json::to_string(&sorted).unwrap();
        assert_eq!(
            json,
            "{\"python_cross_platform\":\"wasix_wasm32\",\"python_extra_index_url\":\"https://pythonindex.wasix.org/simple\",\"python_framework\":\"django\"}"
        );
    }

    #[test]
    fn test_wasmer_app_yaml_updates_existing_volume_with_same_mount() {
        let tmp = tempfile::tempdir().unwrap();
        let runner = make_runner(tmp.path());
        std::fs::write(
            runner.src_dir.join("app.yaml"),
            concat!(
                "volumes:\n",
                "- name: old-wp-content\n",
                "  mount: /app/wp-content\n",
                "  retention: keep\n",
                "- name: cache\n",
                "  mount: /app/cache\n",
            ),
        )
        .unwrap();

        let mut serve = serve(
            "wordpress",
            "wordpress",
            vec![package("php", None, None)],
            None,
            &[("start", "php -S localhost:8080 -t /app")],
        );
        serve.volumes = Some(vec![
            Volume {
                name: "wp-content".to_owned(),
                path: tmp
                    .path()
                    .join(".anybuild")
                    .join("volumes")
                    .join("wp-content"),
                serve_path: PathBuf::from("/app/wp-content"),
            },
            Volume {
                name: "uploads".to_owned(),
                path: tmp.path().join(".anybuild").join("volumes").join("uploads"),
                serve_path: PathBuf::from("/app/uploads"),
            },
        ]);

        runner.build_serve(&serve).unwrap();

        let app_yaml = read_yaml(&runner.wasmer_dir_path.join("app.yaml"));
        let volumes = app_yaml
            .get(yaml_str("volumes"))
            .and_then(YamlValue::as_sequence)
            .unwrap();

        let wp_content: Vec<&YamlValue> = volumes
            .iter()
            .filter(|volume| {
                volume.as_mapping().unwrap().get(yaml_str("mount"))
                    == Some(&yaml_str("/app/wp-content"))
            })
            .collect();
        assert_eq!(wp_content.len(), 1);
        let wp_map = wp_content[0].as_mapping().unwrap();
        assert_eq!(wp_map.len(), 3);
        assert_eq!(wp_map.get(yaml_str("name")), Some(&yaml_str("wp-content")));
        assert_eq!(wp_map.get(yaml_str("retention")), Some(&yaml_str("keep")));

        let find = |name: &str, mount: &str| {
            volumes.iter().any(|volume| {
                let map = volume.as_mapping().unwrap();
                map.get(yaml_str("name")) == Some(&yaml_str(name))
                    && map.get(yaml_str("mount")) == Some(&yaml_str(mount))
                    && map.len() == 2
            })
        };
        assert!(find("cache", "/app/cache"));
        assert!(find("uploads", "/app/uploads"));
    }

    #[test]
    fn test_wasmer_node_manifest_maps_to_edgejs() {
        let tmp = tempfile::tempdir().unwrap();
        let runner = make_runner(tmp.path());

        let serve = serve(
            "node",
            "node",
            vec![package("node", Some("22"), None)],
            Some("/app"),
            &[("start", "node server.js")],
        );

        runner.build_serve(&serve).unwrap();

        let manifest = read_toml(&runner.wasmer_dir_path.join("wasmer.toml"));
        assert_eq!(
            manifest["dependencies"]["wasmer/edgejs-quickjs"].as_str(),
            Some("=0.1.0")
        );
        let command = manifest["command"]
            .as_array_of_tables()
            .unwrap()
            .get(0)
            .unwrap();
        assert_eq!(
            command.get("module").and_then(Item::as_str),
            Some("wasmer/edgejs-quickjs:edge")
        );
        let wasi = command
            .get("annotations")
            .and_then(Item::as_table_like)
            .and_then(|a| a.get("wasi"))
            .and_then(Item::as_table_like)
            .unwrap();
        let main_args: Vec<&str> = wasi
            .get("main-args")
            .and_then(Item::as_array)
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(main_args, vec!["--bytecode-cache", "server.js"]);
        assert!(wasi.get("env").is_none());
    }

    #[test]
    fn test_wasmer_phpix_manifest_maps_php_versions() {
        let cases: &[(Option<&str>, Option<&str>, &str)] = &[
            (None, None, "phpix/phpix-84-32bit"),
            (Some("latest"), Some("64-bit"), "phpix/phpix-84-64bit"),
            (Some("8.5"), Some("32-bit"), "phpix/phpix-85-32bit"),
            (Some("8.5"), Some("64-bit"), "phpix/phpix-85-64bit"),
            (Some("8.4"), Some("32-bit"), "phpix/phpix-84-32bit"),
            (Some("8.4"), Some("64-bit"), "phpix/phpix-84-64bit"),
            (Some("8.3.29"), None, "phpix/phpix-83-32bit"),
        ];
        for (version, architecture, expected_package) in cases {
            let tmp = tempfile::tempdir().unwrap();
            let runner = make_runner(tmp.path());
            let serve = serve(
                "php",
                "php",
                vec![package("phpix", *version, *architecture)],
                Some("/app"),
                &[("start", "phpix -S localhost:8080 -t /app")],
            );
            runner.build_serve(&serve).unwrap();
            let manifest = read_toml(&runner.wasmer_dir_path.join("wasmer.toml"));
            assert_eq!(
                manifest["dependencies"][*expected_package].as_str(),
                Some("=0.3.0-rc.3"),
                "version={version:?} architecture={architecture:?}"
            );
            let command = manifest["command"]
                .as_array_of_tables()
                .unwrap()
                .get(0)
                .unwrap();
            assert_eq!(
                command.get("module").and_then(Item::as_str),
                Some(format!("{expected_package}:phpix").as_str())
            );
        }
    }

    #[test]
    fn test_wasmer_edge_manifest_does_not_duplicate_bytecode_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let runner = make_runner(tmp.path());
        let serve = serve(
            "node",
            "node",
            vec![package("node", Some("22"), None)],
            None,
            &[("start", "edge --bytecode-cache server.js")],
        );
        runner.build_serve(&serve).unwrap();
        let manifest = read_toml(&runner.wasmer_dir_path.join("wasmer.toml"));
        let command = manifest["command"]
            .as_array_of_tables()
            .unwrap()
            .get(0)
            .unwrap();
        let wasi = command
            .get("annotations")
            .and_then(Item::as_table_like)
            .and_then(|a| a.get("wasi"))
            .and_then(Item::as_table_like)
            .unwrap();
        let main_args: Vec<&str> = wasi
            .get("main-args")
            .and_then(Item::as_array)
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(main_args, vec!["--bytecode-cache", "server.js"]);
    }

    #[test]
    fn test_wasmer_prepare_config_enables_node_edge_optimizations() {
        let tmp = tempfile::tempdir().unwrap();
        let mut runner = make_runner(tmp.path());

        let config = runner.prepare_config(ProviderConfig::Node(NodeConfig::default()));

        let runner_json = runner.provider_config.as_ref().unwrap();
        assert_eq!(runner_json["edgejs_enable"], JsonValue::Bool(true));
        assert_eq!(runner_json["edgejs_precompile"], JsonValue::Bool(true));
        assert_eq!(
            runner_json["node_remove_native_binaries"],
            JsonValue::Bool(true)
        );
        let plan_json = config.to_json();
        assert_eq!(plan_json["edgejs_enable"], JsonValue::Bool(true));
        assert_eq!(plan_json["edgejs_precompile"], JsonValue::Bool(true));
        assert_eq!(
            plan_json["node_remove_native_binaries"],
            JsonValue::Bool(true)
        );
    }

    #[test]
    fn test_wasmer_prepare_config_preserves_node_precompile_override() {
        // Exercise the JSON flip directly to cover explicit and null values.
        let mut runner_json = serde_json::json!({ "edgejs_precompile": false });
        apply_runner_flips("node", &mut runner_json);
        assert_eq!(runner_json["edgejs_enable"], JsonValue::Bool(true));
        assert_eq!(runner_json["edgejs_precompile"], JsonValue::Bool(false));
        assert_eq!(
            runner_json["node_remove_native_binaries"],
            JsonValue::Bool(true)
        );

        let mut null_json = serde_json::json!({ "edgejs_precompile": null });
        apply_runner_flips("node", &mut null_json);
        assert_eq!(null_json["edgejs_precompile"], JsonValue::Bool(true));

        let mut nitro_json =
            serde_json::json!({ "node_framework": "nitro", "edgejs_precompile": null });
        apply_runner_flips("node", &mut nitro_json);
        assert_eq!(nitro_json["edgejs_precompile"], JsonValue::Bool(false));

        let mut explicit_nitro_json = serde_json::json!({
            "node_framework": "nitro",
            "edgejs_precompile": true
        });
        apply_runner_flips("node", &mut explicit_nitro_json);
        assert_eq!(
            explicit_nitro_json["edgejs_precompile"],
            JsonValue::Bool(true)
        );
    }

    #[test]
    fn test_wasmer_prepare_config_enables_phpix() {
        for initial_phpix in [false, true] {
            let tmp = tempfile::tempdir().unwrap();
            let mut runner = make_runner(tmp.path());
            let mut php_json = crate::providers::defaults_json("php").unwrap();
            php_json["phpix"] = JsonValue::Bool(initial_phpix);
            let php = crate::providers::config_from_json("php", php_json).unwrap();
            let config = runner.prepare_config(php);

            assert_eq!(config.to_json()["phpix"], JsonValue::Bool(true));
            assert_eq!(
                runner.provider_config.as_ref().unwrap()["phpix"],
                JsonValue::Bool(true)
            );
        }
    }

    #[test]
    fn test_wasmer_prepare_config_enables_wordpress_phpix() {
        let tmp = tempfile::tempdir().unwrap();
        let mut runner = make_runner(tmp.path());

        let config = runner.prepare_config(ProviderConfig::Wordpress(
            crate::providers::wordpress::WordPressConfig::default(),
        ));

        let ProviderConfig::Wordpress(config) = config else {
            panic!("expected wordpress config");
        };
        assert!(config.php.phpix);
        assert_eq!(
            runner.provider_config.as_ref().unwrap()["phpix"],
            JsonValue::Bool(true)
        );
    }

    #[test]
    fn test_wasmer_prepare_config_python_trio_is_plan_visible() {
        let tmp = tempfile::tempdir().unwrap();
        let mut runner = make_runner(tmp.path());
        let config = runner.prepare_config(ProviderConfig::Python(PythonConfig::default()));

        let plan_json = config.to_json();
        assert_eq!(
            plan_json["python_extra_index_url"],
            JsonValue::String("https://pythonindex.wasix.org/simple".to_owned())
        );
        assert_eq!(
            plan_json["python_cross_platform"],
            JsonValue::String("wasix_wasm32".to_owned())
        );
        assert_eq!(plan_json["python_precompile"], JsonValue::Bool(true));
        // The runner copy carries the same trio (deep-copied after the
        // plan-visible mutation).
        let runner_json = runner.provider_config.as_ref().unwrap();
        assert_eq!(
            runner_json["python_cross_platform"],
            JsonValue::String("wasix_wasm32".to_owned())
        );
    }

    #[test]
    fn test_wasmer_run_command_enables_napi_for_edgejs() {
        let tmp = tempfile::tempdir().unwrap();
        let mut runner = make_runner(tmp.path());
        std::fs::create_dir_all(&runner.wasmer_dir_path).unwrap();
        std::fs::write(
            runner.wasmer_dir_path.join("wasmer.toml"),
            concat!(
                "\n[[command]]\n",
                "name = \"start\"\n",
                "module = \"sadhbh-c0d3/edgejs-quickjs:edge\"\n",
                "runner = \"wasi\"\n",
            ),
        )
        .unwrap();

        runner.run_serve_command("start", None, &[], None).unwrap();

        let captured = runner.captured_commands.last().unwrap();
        assert_eq!(captured.command, "wasmer");
        assert_eq!(captured.extra_args[0], "run");
        // assert_eq!(captured.extra_args[1], "--experimental-napi");
        assert!(runner.command_uses_edgejs("start"));
        assert!(runner.has_serve_command("start"));
        assert!(!runner.has_serve_command("missing"));
    }

    #[test]
    fn test_wasmer_run_command_passes_runtime_env() {
        let tmp = tempfile::tempdir().unwrap();
        let mut runner = make_runner(tmp.path());

        let mut env = IndexMap::new();
        env.insert("PORT".to_owned(), "45678".to_owned());
        runner
            .run_serve_command("start", None, &[], Some(&env))
            .unwrap();

        let captured = runner.captured_commands.last().unwrap();
        assert_eq!(captured.command, "wasmer");
        assert_eq!(
            captured.env.as_ref().unwrap().get("PORT"),
            Some(&"45678".to_owned())
        );
        assert!(captured
            .extra_args
            .iter()
            .any(|arg| arg == "--command=start"));
    }

    #[test]
    fn test_wasmer_prepare_mounts_generated_script_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let mut runner = make_runner(tmp.path());
        let prepare_dir = runner.wasmer_dir_path.join("prepare");
        let prepare = [RunStep {
            command: "echo prepare".to_owned(),
            inputs: None,
            outputs: None,
            group: None,
        }];

        runner.prepare(&IndexMap::new(), &prepare).unwrap();

        let captured = runner.captured_commands.last().unwrap();
        let mounted_prepare_dir = prepare_dir.canonicalize().unwrap();
        assert!(captured
            .extra_args
            .iter()
            .any(|arg| arg == &format!("--volume={}:/prepare", mounted_prepare_dir.display())));
        assert!(prepare_dir.is_dir());
    }

    #[test]
    fn test_wasmer_prepare_runs_dependency_binary_directly() {
        let tmp = tempfile::tempdir().unwrap();
        let mut runner = make_runner(tmp.path());
        let mut serve = serve(
            "node",
            "node",
            vec![package("node", Some("22"), None)],
            Some("/app"),
            &[("start", "node server.js")],
        );
        serve.prepare = Some(vec![RunStep {
            command: "edgejs --precompile /app".to_owned(),
            inputs: None,
            outputs: None,
            group: None,
        }]);
        runner.build_serve(&serve).unwrap();

        runner
            .prepare(&IndexMap::new(), serve.prepare.as_deref().unwrap())
            .unwrap();

        let captured = runner.captured_commands.last().unwrap();
        assert!(captured
            .extra_args
            .iter()
            .any(|arg| arg == "--command=__anybuild_prepare_0"));
        assert!(!captured
            .extra_args
            .iter()
            .any(|arg| arg.ends_with(":/prepare")));
    }

    #[test]
    fn test_wasmer_manifest_formatting_matches_tomlkit_structure() {
        // Golden captured from Python tomlkit (structure; string quoting
        // in arrays intentionally differs: basic vs literal strings).
        let tmp = tempfile::tempdir().unwrap();
        let runner = make_runner(tmp.path());
        let serve = serve(
            "node",
            "node",
            vec![package("node", Some("22"), None)],
            Some("/app"),
            &[("start", "node server.js")],
        );
        runner.build_serve(&serve).unwrap();
        let manifest = std::fs::read_to_string(runner.wasmer_dir_path.join("wasmer.toml")).unwrap();
        let expected = format!(
            "# Wasmer manifest generated with Anybuild v{}\n\n\
             [package]\n\
             entrypoint = \"start\"\n\n\
             [dependencies]\n\
             \"wasmer/edgejs-quickjs\" = \"=0.1.0\"\n\n\
             [fs]\n\
             [[command]]\n\
             name = \"start\"\n\
             module = \"wasmer/edgejs-quickjs:edge\"\n\
             runner = \"wasi\"\n\n\
             [command.annotations.wasi]\n\
             cwd = \"/app\"\n\
             main-args = [\n    \"--bytecode-cache\",\n    \"server.js\",\n]\n",
            anybuild_version()
        );
        assert_eq!(manifest, expected);
    }

    #[test]
    fn test_prepare_build_steps_swaps_go_for_go_wasix() {
        let tmp = tempfile::tempdir().unwrap();
        let runner = make_runner(tmp.path());
        let steps = vec![Step::Use(UseStep {
            dependencies: vec![package("go", Some("1.22"), None)],
        })];
        let out = runner.prepare_build_steps(steps);
        assert_eq!(out.len(), 2);
        match &out[0] {
            Step::Use(use_step) => {
                assert_eq!(use_step.dependencies[0].name, "go-wasix");
                assert_eq!(use_step.dependencies[0].version, None);
            }
            other => panic!("expected UseStep, got {other:?}"),
        }
        match &out[1] {
            Step::Env(env_step) => {
                assert_eq!(env_step.variables.get("GOOS"), Some(&"wasip1".to_owned()));
                assert_eq!(env_step.variables.get("GOARCH"), Some(&"wasm".to_owned()));
            }
            other => panic!("expected EnvStep, got {other:?}"),
        }
    }

    /// Port of test_wordpress_phpix.py::
    /// test_wasmer_app_yaml_sets_memory_limit_for_wordpress_phpix.
    #[test]
    fn test_wasmer_app_yaml_sets_memory_limit_for_wordpress_phpix() {
        let tmp = tempfile::tempdir().unwrap();
        let mut runner = make_runner(tmp.path());

        let mut config = crate::providers::wordpress::WordPressConfig::default();
        config.php.phpix = true;
        runner.prepare_config(ProviderConfig::Wordpress(config));

        let mut serve = serve(
            "wordpress",
            "wordpress",
            vec![package("phpix", None, None), package("bash", None, None)],
            Some("/app"),
            &[
                ("start", "phpix -S localhost:8080 -t /app"),
                ("after_deploy", "bash /opt/assets/setup-wp.sh"),
            ],
        );
        serve.env = Some(
            [("PHPIX_PHP_THREADS".to_owned(), "4".to_owned())]
                .into_iter()
                .collect(),
        );
        serve.services = Some(vec![crate::plan::Service {
            name: "database".to_owned(),
            provider: "mysql".to_owned(),
        }]);

        runner.build_serve(&serve).unwrap();

        let app_yaml = read_yaml(&runner.wasmer_dir_path.join("app.yaml"));
        let capabilities = app_yaml
            .get(yaml_str("capabilities"))
            .and_then(YamlValue::as_mapping)
            .unwrap();
        assert_eq!(
            capabilities
                .get(yaml_str("database"))
                .and_then(YamlValue::as_mapping)
                .and_then(|db| db.get(yaml_str("engine")))
                .cloned(),
            Some(yaml_str("mysql"))
        );
        assert_eq!(
            capabilities
                .get(yaml_str("memory"))
                .and_then(YamlValue::as_mapping)
                .and_then(|memory| memory.get(yaml_str("limit")))
                .cloned(),
            Some(yaml_str("2Gb"))
        );
        assert_eq!(
            app_yaml.get(yaml_str("enable_email")),
            Some(&YamlValue::Bool(true))
        );
        let env = app_yaml
            .get(yaml_str("env"))
            .and_then(YamlValue::as_mapping)
            .unwrap();
        assert_eq!(env.get(yaml_str("PHPIX_PHP_THREADS")), Some(&yaml_str("4")));
        let annotations = app_yaml
            .get(yaml_str("annotations"))
            .and_then(YamlValue::as_mapping)
            .unwrap();
        assert_eq!(
            annotations.get(yaml_str("wasmer.io/app-kind")),
            Some(&yaml_str("wordpress"))
        );
    }
}
