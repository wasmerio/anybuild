//! Static Node provider implementation.
//!
//! `NodeStaticConfig(NodeConfig, StaticFileConfig)` in Python — the
//! staticfile-side fields and the slice of `StaticFileProvider` behavior
//! it consumes (`_load_static_config`, `compute_redirects_config`) are
//! carried privately here so this module stays self-contained.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::LazyLock;

use anyhow::{anyhow, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::operation::OperationContext;
use crate::providers::base::{env_bool, env_str, BaseConfig, HasBase};
use crate::providers::install_context::{
    discover_js_install_context, read_json_object, yaml_scalar,
};
use crate::providers::node::{
    self, JsonMap, NodeConfig, NodeConfigFields, NodeFramework, PackageManager,
};
use crate::providers::staticfile::compute_redirects_config;
use crate::providers::{workspace, Provider};

// ---------------------------------------------------------------------------
// Config

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NodeStaticConfig {
    #[serde(flatten)]
    pub base: BaseConfig,
    // StaticFileConfig fields.
    pub convert_redirects: bool,
    pub sws_version: Option<String>,
    pub static_dir: Option<String>,
    /// Rendered sws.toml redirects (from a _redirects file), computed at
    /// load time so the Starlark provider stays filesystem-free.
    pub redirects_config: Option<String>,
    #[serde(flatten)]
    pub node: NodeConfigFields,
}

impl Default for NodeStaticConfig {
    fn default() -> Self {
        let node = NodeConfig::default();
        Self {
            base: node.base,
            convert_redirects: true,
            sws_version: Some("2.38.0".to_owned()),
            static_dir: None,
            redirects_config: None,
            node: node.node,
        }
    }
}

impl NodeStaticConfig {
    /// pydantic-settings construction: node fields plus the staticfile
    /// fields, each overlaid from `ANYBUILD_<FIELD>`.
    fn from_env(base: BaseConfig, operation: &OperationContext) -> Result<Self> {
        let node = NodeConfig::from_env(base, operation)?;
        Ok(Self {
            base: node.base,
            convert_redirects: env_bool(operation, "convert_redirects")?.unwrap_or(true),
            sws_version: env_str(operation, "sws_version").or_else(|| Some("2.38.0".to_owned())),
            static_dir: env_str(operation, "static_dir"),
            redirects_config: env_str(operation, "redirects_config"),
            node: node.node,
        })
    }
}

impl std::ops::Deref for NodeStaticConfig {
    type Target = NodeConfigFields;

    fn deref(&self) -> &Self::Target {
        &self.node
    }
}

impl std::ops::DerefMut for NodeStaticConfig {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.node
    }
}

impl HasBase for NodeStaticConfig {
    fn base(&self) -> &BaseConfig {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseConfig {
        &mut self.base
    }
}

// ---------------------------------------------------------------------------
// Dependency lists

const SCRIPT_BUILD_COMMAND: &[&str] = &["build"];
// Only use these commands if the build command is not found in the
// package.json.
const SCRIPT_BUILD_COMMAND_FALLBACK: &[&str] = &["generate", "export", "docs:build"];

const PURE_STATIC_DEPENDENCIES: &[&str] = &[
    "@angular/cli",
    "@11ty/eleventy",
    "@ionic/angular",
    "@ionic/react",
    "@stencil/core",
    "@vue/cli-service",
    "brunch",
    "ember-cli",
    "ember-source",
    "vitepress",
    "vuepress",
    "hexo",
    "hexo-cli",
    "metalsmith",
    "assemble",
    "grunt-assemble",
    "harp",
    "parcel",
    "polymer-cli",
    "preact-cli",
    "docusaurus",
    "@docusaurus/core",
    "react-scripts",
    "umi",
    "@sveltejs/kit",
    "sanity",
    "storybook",
];

const STATIC_DEPENDENCIES: &[&str] = &[
    "astro",
    "vite",
    "next",
    "nuxt",
    "gatsby",
    "svelte",
    "@remix-run/dev",
];

static STATIC_FRAMEWORK_DEPENDENCIES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    PURE_STATIC_DEPENDENCIES
        .iter()
        .chain(STATIC_DEPENDENCIES.iter())
        .copied()
        .chain(["@remix-run/vite"])
        .collect()
});

const RUNTIME_DEPENDENCIES: &[&str] = &[
    "@astrojs/node",
    "@sveltejs/adapter-node",
    "@react-router/dev",
    "@react-router/serve",
    "@remix-run/serve",
    "@tanstack/react-start",
    "@solidjs/start",
    "solid-start",
    "nitropack",
    "@shopify/hydrogen",
    "@shopify/remix-oxygen",
    "@redwoodjs/core",
    "@elysia/node",
    "elysia",
];

static STATIC_DETECT_DEPENDENCIES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    STATIC_FRAMEWORK_DEPENDENCIES
        .iter()
        .chain(RUNTIME_DEPENDENCIES.iter())
        .copied()
        .chain(["@remix-run/node"])
        .collect()
});

static ASSEMBLE_DEST_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\bdest\s*:\s*['"]([^'"]+)['"]"#).expect("valid regex"));

static NEXT_STATIC_EXPORT_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\boutput\s*:\s*['"]export['"]"#).expect("valid regex"));

// ---------------------------------------------------------------------------
// load_config

pub fn load_config(
    path: &Path,
    base: BaseConfig,
    operation: &OperationContext,
) -> Result<NodeStaticConfig> {
    // StaticFileProvider.load_config(path, base_config), whose dump is
    // merged into the NodeStaticConfig construction.
    let static_parts = load_static_parts(path, operation)?;
    let mut config = NodeStaticConfig::from_env(base, operation)?;
    config.convert_redirects = static_parts.convert_redirects;
    config.sws_version = static_parts.sws_version;
    config.static_dir = static_parts.static_dir;
    config.redirects_config = static_parts.redirects_config;

    let package_manager = config
        .package_manager
        .unwrap_or_else(|| node::detect_package_manager(path));
    config.package_manager = Some(package_manager);

    let package_json = node::parse_package_json(path);
    let found_deps =
        node::check_package_json_deps(package_json.as_ref(), &STATIC_FRAMEWORK_DEPENDENCIES);

    if config.framework.is_none() {
        config.framework = detect_static_framework(path, package_json.as_ref(), &found_deps);
    }

    if node::non_empty(&config.build_command).is_none() {
        config.build_command = get_build_command(
            package_json.as_ref(),
            package_manager,
            config.framework,
            None,
        );
    }

    if node::non_empty(&config.static_dir).is_none() {
        config.static_dir = Some(match config.framework {
            Some(framework) => get_static_dir(path, package_json.as_ref(), framework)?,
            None => "dist".to_owned(),
        });
    }

    let install_context = discover_js_install_context(path);
    if install_context.requires_all_files {
        config.install_requires_all_files = true;
    }
    // NodeProvider.apply_static_snapshot
    config.install_inputs = Some(install_context.inputs);
    if let Some(name) = node::package_json_name(package_json.as_ref()) {
        config.package_name = Some(name);
    }

    // static_dir may have changed since the base load; recompute redirects.
    config.redirects_config =
        compute_redirects_config(path, config.static_dir.as_deref(), config.convert_redirects)?;

    Ok(config)
}

/// The staticfile-side fields produced by `StaticFileProvider.load_config`
/// (env overlay included, masked where Python passes explicit kwargs).
struct StaticParts {
    convert_redirects: bool,
    sws_version: Option<String>,
    static_dir: Option<String>,
    redirects_config: Option<String>,
}

fn load_static_parts(path: &Path, operation: &OperationContext) -> Result<StaticParts> {
    let mut parts = StaticParts {
        convert_redirects: env_bool(operation, "convert_redirects")?.unwrap_or(true),
        sws_version: env_str(operation, "sws_version").or_else(|| Some("2.38.0".to_owned())),
        static_dir: env_str(operation, "static_dir"),
        redirects_config: env_str(operation, "redirects_config"),
    };

    // StaticFileProvider._load_static_config
    let staticfile_path = path.join("Staticfile");
    let mut handled = false;
    if staticfile_path.exists() {
        if let Some(root) = staticfile_config_root(&staticfile_path) {
            // StaticFileConfig(**base, static_dir=config.get("root")) —
            // the explicit kwarg masks the env overlay, even when None.
            parts.static_dir = root;
            handled = true;
        }
    }
    if !handled
        && (path.join("public/index.html").exists() || path.join("public/index.htm").exists())
    {
        parts.static_dir = Some("public".to_owned());
    }

    parts.redirects_config =
        compute_redirects_config(path, parts.static_dir.as_deref(), parts.convert_redirects)?;
    Ok(parts)
}

/// Minimal Staticfile (YAML mapping) read: `Some(root)` when the file
/// parses to a truthy config, carrying its optional `root:` value.
fn staticfile_config_root(path: &Path) -> Option<Option<String>> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut any_key = false;
    let mut root: Option<String> = None;
    for raw_line in text.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            any_key = true;
            if key.trim() == "root" {
                let value = yaml_scalar(value).unwrap_or_default();
                root = if value.is_empty() { None } else { Some(value) };
            }
        }
    }
    if any_key {
        Some(root)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Framework detection

fn detect_static_framework(
    path: &Path,
    package_json: Option<&JsonMap>,
    found_deps: &BTreeSet<&'static str>,
) -> Option<NodeFramework> {
    let any = |deps: &[&str]| deps.iter().any(|dep| found_deps.contains(dep));

    if found_deps.contains("harp") {
        return Some(NodeFramework::Harp);
    }
    if found_deps.contains("gatsby") {
        return Some(NodeFramework::Gatsby);
    }
    if found_deps.contains("astro") {
        return Some(NodeFramework::Astro);
    }
    if found_deps.contains("next") {
        return Some(NodeFramework::Next);
    }
    if found_deps.contains("nuxt") {
        if node::has_dependency(package_json, "nuxt", Some("2"))
            || node::has_dependency(package_json, "nuxt", Some("1"))
        {
            return Some(NodeFramework::NuxtOld);
        }
        return Some(NodeFramework::NuxtV3);
    }
    if found_deps.contains("vitepress") {
        return Some(NodeFramework::Vitepress);
    }
    if found_deps.contains("vuepress") {
        return Some(NodeFramework::Vuepress);
    }
    if any(&["hexo", "hexo-cli"]) {
        return Some(NodeFramework::Hexo);
    }
    if found_deps.contains("metalsmith") {
        return Some(NodeFramework::Metalsmith);
    }
    if any(&["assemble", "grunt-assemble"]) {
        return Some(NodeFramework::Assemble);
    }
    if found_deps.contains("docusaurus") {
        return Some(NodeFramework::DocusaurusOld);
    }
    if found_deps.contains("@docusaurus/core") {
        return Some(NodeFramework::Docusaurus);
    }
    if found_deps.contains("sanity") {
        if node::has_dependency_major(package_json, "sanity", 3) {
            return Some(NodeFramework::SanityV3);
        }
        return Some(NodeFramework::Sanity);
    }
    if found_deps.contains("@ionic/angular") {
        return Some(NodeFramework::IonicAngular);
    }
    if found_deps.contains("@ionic/react") {
        return Some(NodeFramework::IonicReact);
    }
    if found_deps.contains("@angular/cli") {
        return Some(NodeFramework::Angular);
    }
    if found_deps.contains("react-scripts") {
        return Some(NodeFramework::CreateReactApp);
    }
    if found_deps.contains("brunch") {
        return Some(NodeFramework::Brunch);
    }
    if any(&["ember-cli", "ember-source"]) {
        return Some(NodeFramework::Ember);
    }
    if found_deps.contains("parcel") {
        return Some(NodeFramework::Parcel);
    }
    if found_deps.contains("polymer-cli") {
        return Some(NodeFramework::Polymer);
    }
    if found_deps.contains("preact-cli") {
        return Some(NodeFramework::Preact);
    }
    if found_deps.contains("@stencil/core") {
        return Some(NodeFramework::Stencil);
    }
    if found_deps.contains("umi") {
        return Some(NodeFramework::Umijs);
    }
    if found_deps.contains("@vue/cli-service") {
        return Some(NodeFramework::Vue);
    }
    if found_deps.contains("@11ty/eleventy") {
        return Some(NodeFramework::Eleventy);
    }
    if found_deps.contains("@sveltejs/kit") {
        return Some(NodeFramework::Sveltekit);
    }
    if found_deps.contains("svelte") {
        return Some(NodeFramework::Svelte);
    }
    if found_deps.contains("@remix-run/dev") {
        if node::has_dependency(package_json, "@remix-run/dev", Some("1"))
            || node::has_dependency(package_json, "@remix-run/dev", Some("0"))
        {
            return Some(NodeFramework::RemixOld);
        }
        if has_vite_remix(path, found_deps) {
            return Some(NodeFramework::RemixV2);
        }
        return Some(NodeFramework::RemixV2Classic);
    }
    if found_deps.contains("vite") {
        return Some(NodeFramework::Vite);
    }
    if found_deps.contains("storybook") {
        return Some(NodeFramework::Storybook);
    }
    None
}

fn has_vite_remix(path: &Path, found_deps: &BTreeSet<&'static str>) -> bool {
    found_deps.contains("@remix-run/vite")
        || found_deps.contains("vite")
        || [
            "vite.config.js",
            "vite.config.ts",
            "vite.config.mjs",
            "vite.config.cjs",
        ]
        .iter()
        .any(|file| path.join(file).exists())
}

// ---------------------------------------------------------------------------
// Static dir resolution

fn get_static_dir(
    path: &Path,
    package_json: Option<&JsonMap>,
    framework: NodeFramework,
) -> Result<String> {
    let default_dir = || {
        framework
            .get_static_output_dir()
            .ok_or_else(|| {
                anyhow!("framework {framework:?} does not have a static output directory")
            })
            .map(str::to_owned)
    };

    Ok(match framework {
        NodeFramework::Angular | NodeFramework::IonicAngular => {
            angular_output_dir(path).map_or_else(default_dir, Ok)?
        }
        NodeFramework::Vitepress => {
            let root = script_build_root(package_json, "vitepress")
                .unwrap_or_else(|| default_docs_root(path, ".vitepress"));
            rooted_output_dir(&root, ".vitepress/dist")
        }
        NodeFramework::Vuepress => {
            let root = script_build_root(package_json, "vuepress")
                .unwrap_or_else(|| default_docs_root(path, ".vuepress"));
            rooted_output_dir(&root, ".vuepress/dist")
        }
        NodeFramework::Metalsmith => metalsmith_output_dir(path).map_or_else(default_dir, Ok)?,
        NodeFramework::Assemble => assemble_output_dir(path).map_or_else(default_dir, Ok)?,
        NodeFramework::Harp => harp_output_dir(package_json).map_or_else(default_dir, Ok)?,
        _ => default_dir()?,
    })
}

/// node_static's `_script_commands` override: build scripts first, then
/// the generate/export/docs:build fallback.
fn static_script_commands<'a>(
    package_json: Option<&'a JsonMap>,
    preferred: &[&str],
) -> Vec<&'a str> {
    let commands = node::script_commands(package_json, preferred);
    if commands.is_empty() {
        // Only use these commands if the build command is not found in
        // the package.json.
        return node::script_commands(package_json, SCRIPT_BUILD_COMMAND_FALLBACK);
    }
    commands
}

fn detect_script_commands(package_json: Option<&JsonMap>) -> Vec<&str> {
    let mut commands = static_script_commands(package_json, SCRIPT_BUILD_COMMAND);
    for command in node::script_commands(package_json, SCRIPT_BUILD_COMMAND_FALLBACK) {
        if !commands.contains(&command) {
            commands.push(command);
        }
    }
    commands
}

fn args_after_command(command: &str, executable: &str) -> Vec<String> {
    let tokens = node::split_command(command);
    for (index, token) in tokens.iter().enumerate() {
        if token == executable {
            return tokens[index + 1..].to_vec();
        }
    }
    Vec::new()
}

fn script_build_root(package_json: Option<&JsonMap>, executable: &str) -> Option<String> {
    for command in static_script_commands(package_json, SCRIPT_BUILD_COMMAND) {
        let args = args_after_command(command, executable);
        if args.first().map(String::as_str) != Some("build") {
            continue;
        }
        for arg in &args[1..] {
            if !arg.starts_with('-') {
                return Some(clean_output_dir(arg));
            }
        }
    }
    None
}

fn default_docs_root(path: &Path, config_dir: &str) -> String {
    let docs_path = path.join("docs");
    if docs_path.join(config_dir).exists() || docs_path.exists() {
        "docs".to_owned()
    } else {
        ".".to_owned()
    }
}

fn rooted_output_dir(root: &str, output_dir: &str) -> String {
    let root = clean_output_dir(root);
    if root == "." {
        output_dir.to_owned()
    } else {
        format!("{root}/{output_dir}")
    }
}

fn clean_output_dir(output_dir: &str) -> String {
    let mut output_dir = output_dir.trim().trim_end_matches('/');
    if let Some(rest) = output_dir.strip_prefix("./") {
        output_dir = rest;
    }
    if output_dir.is_empty() {
        ".".to_owned()
    } else {
        output_dir.to_owned()
    }
}

fn metalsmith_output_dir(path: &Path) -> Option<String> {
    for config_name in ["metalsmith.json", ".metalsmith.json"] {
        let config_path = path.join(config_name);
        if !config_path.is_file() {
            continue;
        }
        let Some(config) = read_json_object(&config_path) else {
            continue;
        };
        for key in ["destination", "dest"] {
            if let Some(output_dir) = config.get(key).and_then(Value::as_str) {
                if !output_dir.is_empty() {
                    return Some(clean_output_dir(output_dir));
                }
            }
        }
    }
    None
}

fn assemble_output_dir(path: &Path) -> Option<String> {
    for config_name in ["Gruntfile.js", "Gruntfile.cjs"] {
        let config_path = path.join(config_name);
        if !config_path.is_file() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&config_path) else {
            continue;
        };
        if let Some(captures) = ASSEMBLE_DEST_PATTERN.captures(&text) {
            return Some(clean_output_dir(&captures[1]));
        }
    }
    None
}

fn harp_output_dir(package_json: Option<&JsonMap>) -> Option<String> {
    for command in static_script_commands(package_json, SCRIPT_BUILD_COMMAND) {
        let mut args = args_after_command(command, "harp");
        if args.is_empty() {
            continue;
        }
        if args[0] == "compile" {
            args.remove(0);
        }
        let mut positional_args: Vec<&str> = Vec::new();
        for (index, arg) in args.iter().enumerate() {
            if (arg == "--output" || arg == "-o") && index + 1 < args.len() {
                return Some(clean_output_dir(&args[index + 1]));
            }
            if !arg.starts_with('-') {
                positional_args.push(arg);
            }
        }
        if positional_args.len() >= 2 {
            return Some(clean_output_dir(positional_args[1]));
        }
    }
    None
}

fn angular_output_dir(path: &Path) -> Option<String> {
    let config_path = path.join("angular.json");
    if !config_path.is_file() {
        return None;
    }
    let config = read_json_object(&config_path)?;

    let Some(Value::Object(projects)) = config.get("projects") else {
        return None;
    };

    let mut project_names: Vec<&str> = Vec::new();
    if let Some(default_project) = config.get("defaultProject").and_then(Value::as_str) {
        if projects.contains_key(default_project) {
            project_names.push(default_project);
        }
    }
    for name in projects.keys() {
        if !project_names.contains(&name.as_str()) {
            project_names.push(name);
        }
    }

    for project_name in project_names {
        let Some(Value::Object(project)) = projects.get(project_name) else {
            continue;
        };
        for target_root in ["architect", "targets"] {
            let Some(Value::Object(targets)) = project.get(target_root) else {
                continue;
            };
            let Some(Value::Object(build_target)) = targets.get("build") else {
                continue;
            };
            let Some(Value::Object(options)) = build_target.get("options") else {
                continue;
            };
            if let Some(output_dir) = angular_output_path(options.get("outputPath")) {
                return Some(output_dir);
            }
        }
    }
    None
}

fn angular_output_path(output_path: Option<&Value>) -> Option<String> {
    match output_path {
        Some(Value::String(value)) if !value.is_empty() => Some(clean_output_dir(value)),
        Some(Value::Object(map)) => {
            for key in ["browser", "base"] {
                if let Some(value) = map.get(key).and_then(Value::as_str) {
                    if !value.is_empty() {
                        return Some(clean_output_dir(value));
                    }
                }
            }
            None
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Detection

fn has_runtime_dependency(found_deps: &BTreeSet<&'static str>) -> bool {
    found_deps
        .iter()
        .any(|dep| RUNTIME_DEPENDENCIES.contains(dep))
}

fn has_next_static_export_config(path: &Path) -> bool {
    for config_name in [
        "next.config.js",
        "next.config.cjs",
        "next.config.mjs",
        "next.config.ts",
    ] {
        let config_path = path.join(config_name);
        if !config_path.is_file() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&config_path) else {
            continue;
        };
        if NEXT_STATIC_EXPORT_PATTERN.is_match(&text) {
            return true;
        }
    }
    false
}

fn has_static_remix_output(path: &Path, package_json: Option<&JsonMap>) -> bool {
    let scripts = node::package_scripts(package_json);
    let start_command = scripts.get("start").copied().unwrap_or("");
    path.join("public").join("index.html").is_file()
        && start_command.contains("serve")
        && start_command.contains("public")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DetectionEvidence {
    Strong,
    Weak,
}

impl Provider for NodeStaticConfig {
    type Evidence = DetectionEvidence;

    const NAME: &'static str = "node-static";
    const DETECTION_DETAILS: &'static [(&'static str, &'static str)] = &[
        ("Framework", "framework"),
        ("Package manager", "package_manager"),
        ("Output directory", "static_dir"),
    ];

    fn format_detection_detail(field: &str, value: &str) -> String {
        if field == "framework" {
            node::display_framework(value)
        } else {
            value.to_owned()
        }
    }

    fn apply_workspace_config(&mut self, workspace_root: &Path) {
        workspace::apply_node_workspace_config(
            workspace_root,
            self.base.app_subdir.as_deref(),
            &mut self.node.package_manager,
            &mut self.node.build_command,
            &mut self.base.commands,
        );
    }

    fn detection_evidence(
        path: &Path,
        base: &BaseConfig,
        _operation: &OperationContext,
    ) -> Option<Self::Evidence> {
        let package_json = node::parse_package_json(path);
        let found_deps =
            node::check_package_json_deps(package_json.as_ref(), &STATIC_DETECT_DEPENDENCIES);

        if has_runtime_dependency(&found_deps) || node::has_hydrogen_config(Some(path)) {
            return None;
        }
        if found_deps.contains("@remix-run/node")
            && !has_static_remix_output(path, package_json.as_ref())
        {
            return None;
        }

        let mut has_package_manager_build_command = false;
        if let Some(build) = node::non_empty(&base.commands.build) {
            // Iterate over all generators and check if the build command
            // matches.
            for framework in NodeFramework::ALL {
                if !framework.can_be_static() {
                    continue;
                }
                let command = framework
                    .build_static_command()
                    .expect("static frameworks have build commands");
                if build.contains(command) {
                    return Some(DetectionEvidence::Strong);
                }
            }

            let frameworks = NodeFramework::detect_from_command(build);
            if !frameworks.is_empty() && !frameworks.contains(&NodeFramework::Next) {
                return Some(DetectionEvidence::Strong);
            }

            has_package_manager_build_command = node::is_package_manager_build_command(build);
        }

        if found_deps.contains("next") && has_next_static_export_config(path) {
            return Some(DetectionEvidence::Strong);
        }

        for build_command in detect_script_commands(package_json.as_ref()) {
            for framework in NodeFramework::ALL {
                if !framework.can_be_static() {
                    continue;
                }
                let command = framework
                    .build_static_command()
                    .expect("static frameworks have build commands");
                if build_command.contains(command) {
                    return Some(DetectionEvidence::Strong);
                }
            }
            let all_frameworks = NodeFramework::detect_from_command(build_command);
            if !all_frameworks.is_empty()
                && all_frameworks
                    .iter()
                    .all(|framework| framework.is_pure_static())
            {
                return Some(DetectionEvidence::Strong);
            }
        }

        let pure_static_deps: BTreeSet<&str> = found_deps
            .iter()
            .copied()
            .filter(|dep| PURE_STATIC_DEPENDENCIES.contains(dep))
            .collect();
        let static_deps: BTreeSet<&str> = found_deps
            .iter()
            .copied()
            .filter(|dep| STATIC_DEPENDENCIES.contains(dep))
            .collect();

        if !pure_static_deps.is_empty()
            && static_deps.difference(&pure_static_deps).next().is_none()
        {
            return Some(DetectionEvidence::Strong);
        }

        if !static_deps.is_empty() {
            return Some(DetectionEvidence::Weak);
        }
        if has_package_manager_build_command {
            return Some(DetectionEvidence::Weak);
        }
        None
    }

    fn load(path: &Path, base: BaseConfig, operation: &OperationContext) -> Result<Self> {
        load_config(path, base, operation)
    }
}

// ---------------------------------------------------------------------------
// Build command

fn get_build_command(
    package_json: Option<&JsonMap>,
    package_manager: PackageManager,
    framework: Option<NodeFramework>,
    explicit_build_command: Option<&str>,
) -> Option<String> {
    if let Some(explicit) = explicit_build_command {
        return Some(explicit.to_owned());
    }
    if let Some(package_json) = package_json {
        let scripts = match package_json.get("scripts") {
            Some(Value::Object(scripts)) => Some(scripts),
            _ => None,
        };
        let script_truthy = |name: &str| {
            scripts
                .and_then(|scripts| scripts.get(name))
                .is_some_and(value_truthy)
        };
        let docs_build = script_truthy("docs:build");
        if matches!(
            framework,
            Some(NodeFramework::Vitepress) | Some(NodeFramework::Vuepress)
        ) && docs_build
        {
            return Some(package_manager.run_command("docs:build"));
        }
        if script_truthy("generate") {
            return Some(package_manager.run_command("generate"));
        }
        if script_truthy("build") {
            return Some(package_manager.run_command("build"));
        }
        if docs_build {
            return Some(package_manager.run_command("docs:build"));
        }
    }
    let framework = framework?;
    let command = framework.build_static_command()?;
    Some(package_manager.run_execute_command(command))
}

fn value_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(s) => !s.is_empty(),
        Value::Number(n) => n.as_f64().is_none_or(|f| f != 0.0),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    //! Port of `tests/test_node_static_provider.py`.
    //!
    //! Not ported:
    //! - `test_node_static_rejects_non_static_framework_config` and
    //!   `test_node_static_rejects_non_static_framework_config_override`:
    //!   pydantic's `framework` validator ("<x> cannot be generated
    //!   statically") has no counterpart in the Rust config layer —
    //!   `NodeStaticConfig` construction / `config_from_json` perform no
    //!   such validation.

    use std::path::PathBuf;

    use super::*;
    use crate::providers::base::BaseConfig;

    fn detect(path: &Path, base: &BaseConfig) -> Option<i32> {
        crate::providers::detection_score_for_test("node-static", path, base)
    }

    fn load_config(path: &Path, base: BaseConfig) -> Result<NodeStaticConfig> {
        super::load_config(path, base, &OperationContext::for_test())
    }

    fn example(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .join(name)
    }

    fn write(path: &Path, contents: &str) {
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn test_new_static_builder_examples_are_pure_static() {
        for (example_name, framework, static_dir, build_command) in [
            (
                "nodestatic-astro",
                NodeFramework::Astro,
                "dist",
                "npm run build",
            ),
            (
                "nodestatic-gatsby",
                NodeFramework::Gatsby,
                "public",
                "npm run build",
            ),
            (
                "nodestatic-next",
                NodeFramework::Next,
                "out",
                "npm run build",
            ),
            (
                "nodestatic-nuxt",
                NodeFramework::NuxtV3,
                ".output/public",
                "npm run generate",
            ),
            (
                "nodestatic-docusaurus",
                NodeFramework::Docusaurus,
                "build",
                "npm run build",
            ),
            (
                "nodestatic-svelte",
                NodeFramework::Sveltekit,
                "build",
                "npm run build",
            ),
            (
                "nodestatic-sveltekit",
                NodeFramework::Sveltekit,
                "build",
                "npm run build",
            ),
            (
                "nodestatic-remix",
                NodeFramework::RemixV2Classic,
                "public",
                "npm run build",
            ),
            (
                "nodestatic-eleventy",
                NodeFramework::Eleventy,
                "_site",
                "npm run build",
            ),
            (
                "nodestatic-vitepress",
                NodeFramework::Vitepress,
                "docs/.vitepress/dist",
                "npm run docs:build",
            ),
            (
                "nodestatic-vuepress",
                NodeFramework::Vuepress,
                "docs/.vuepress/dist",
                "npm run docs:build",
            ),
            (
                "nodestatic-hexo",
                NodeFramework::Hexo,
                "public",
                "npm run generate",
            ),
            (
                "nodestatic-metalsmith",
                NodeFramework::Metalsmith,
                "build",
                "npm run build",
            ),
            (
                "nodestatic-assemble",
                NodeFramework::Assemble,
                "dist",
                "npm run build",
            ),
            (
                "nodestatic-harp",
                NodeFramework::Harp,
                "www",
                "npm run build",
            ),
            (
                "nodestatic-angular",
                NodeFramework::Angular,
                "dist/angular-test",
                "npm run build",
            ),
            (
                "nodestatic-brunch",
                NodeFramework::Brunch,
                "public",
                "npm run build",
            ),
            (
                "nodestatic-create-react-app",
                NodeFramework::CreateReactApp,
                "build",
                "npm run build",
            ),
            (
                "nodestatic-docusaurus-old",
                NodeFramework::DocusaurusOld,
                "build",
                "npm run build",
            ),
            (
                "nodestatic-ember",
                NodeFramework::Ember,
                "dist",
                "npm run build",
            ),
            (
                "nodestatic-ionic-angular",
                NodeFramework::IonicAngular,
                "www",
                "npm run build",
            ),
            (
                "nodestatic-ionic-react",
                NodeFramework::IonicReact,
                "dist",
                "npm run build",
            ),
            (
                "nodestatic-parcel",
                NodeFramework::Parcel,
                "dist",
                "npm run build",
            ),
            (
                "nodestatic-polymer",
                NodeFramework::Polymer,
                "build/default",
                "npm run build",
            ),
            (
                "nodestatic-preact",
                NodeFramework::Preact,
                "build",
                "npm run build",
            ),
            (
                "nodestatic-stencil",
                NodeFramework::Stencil,
                "www",
                "npm run build",
            ),
            (
                "nodestatic-umijs",
                NodeFramework::Umijs,
                "dist",
                "npm run build",
            ),
            (
                "nodestatic-vite",
                NodeFramework::Vite,
                "dist",
                "npm run build",
            ),
            (
                "nodestatic-vite-react",
                NodeFramework::Vite,
                "dist",
                "npm run build",
            ),
            (
                "nodestatic-vue",
                NodeFramework::Vue,
                "dist",
                "npm run build",
            ),
            (
                "nodestatic-sanity",
                NodeFramework::SanityV3,
                "dist",
                "npm run build",
            ),
            (
                "nodestatic-storybook",
                NodeFramework::Storybook,
                "storybook-static",
                "npm run build",
            ),
        ] {
            let path = example(example_name);
            let base = BaseConfig::default();

            let detect_result = detect(&path, &base).expect(example_name);

            assert_eq!(detect_result, 60, "{example_name}");
            assert_eq!(
                crate::providers::load_provider_for_test(&path, &base, None).unwrap(),
                "node-static",
                "{example_name}"
            );

            let config = load_config(&path, base).unwrap();
            assert_eq!(config.framework, Some(framework), "{example_name}");
            assert_eq!(
                config.static_dir.as_deref(),
                Some(static_dir),
                "{example_name}"
            );
            assert_eq!(
                config.build_command.as_deref(),
                Some(build_command),
                "{example_name}"
            );
        }
    }

    #[test]
    fn test_pure_static_dependency_keeps_priority_with_package_script_command() {
        let path = example("nodestatic-vitepress");
        let mut base = BaseConfig::default();
        base.commands.build = Some("npm run docs:build".to_owned());

        let detect_result = detect(&path, &base).expect("detects");

        assert_eq!(detect_result, 60);
    }

    #[test]
    fn test_next_build_without_start_command_uses_node_static() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("package.json"),
            "{\n  \"scripts\": {\n    \"build\": \"next build\"\n  },\n  \"dependencies\": {\n    \"next\": \"^14.2.14\"\n  }\n}\n",
        );
        let mut base = BaseConfig::default();
        base.commands.build = Some("next build".to_owned());

        let detect_result = detect(tmp.path(), &base).expect("detects");
        let node_result = crate::providers::detection_score_for_test("node", tmp.path(), &base)
            .expect("node detects");

        assert_eq!(detect_result, 20);
        assert!(node_result < detect_result);
        assert_eq!(
            crate::providers::load_provider_for_test(tmp.path(), &base, None).unwrap(),
            "node-static"
        );
    }

    #[test]
    fn test_explicit_next_export_command_stays_node_static() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("package.json"),
            "{\n  \"scripts\": {\n    \"build\": \"next export\"\n  },\n  \"dependencies\": {\n    \"next\": \"^14.2.14\"\n  }\n}\n",
        );
        let mut base = BaseConfig::default();
        base.commands.build = Some("next export".to_owned());

        let detect_result = detect(tmp.path(), &base).expect("detects");

        assert_eq!(detect_result, 60);
        assert_eq!(
            crate::providers::load_provider_for_test(tmp.path(), &base, None).unwrap(),
            "node-static"
        );
    }

    #[test]
    fn test_next_output_export_config_uses_node_static() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("package.json"),
            "{\n  \"scripts\": {\n    \"build\": \"next export\"\n  },\n  \"dependencies\": {\n    \"next\": \"^14.2.14\"\n  }\n}\n",
        );
        write(
            &tmp.path().join("next.config.mjs"),
            "const nextConfig = {\n  output: \"export\",\n};\n\nexport default nextConfig;\n",
        );

        let config = load_config(tmp.path(), BaseConfig::default()).unwrap();

        assert_eq!(
            crate::providers::load_provider_for_test(tmp.path(), &BaseConfig::default(), None)
                .unwrap(),
            "node-static"
        );
        assert_eq!(config.framework, Some(NodeFramework::Next));
        assert_eq!(config.static_dir.as_deref(), Some("out"));
        assert_eq!(config.build_command.as_deref(), Some("npm run build"));
    }

    #[test]
    fn test_elysia_dependency_uses_node_provider() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("package.json"),
            "{\n  \"scripts\": {\n    \"build\": \"vite build\",\n    \"start\": \"node server.js\"\n  },\n  \"dependencies\": {\n    \"@elysia/node\": \"^1.4.6\",\n    \"elysia\": \"^1.4.28\",\n    \"vite\": \"^7.2.4\"\n  }\n}\n",
        );

        assert!(detect(tmp.path(), &BaseConfig::default()).is_none());
        assert_ne!(
            crate::providers::load_provider_for_test(tmp.path(), &BaseConfig::default(), None)
                .unwrap(),
            "node-static"
        );
    }

    #[test]
    fn test_nuxt_generate_fallback_uses_node_static() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("package.json"),
            "{\n  \"scripts\": {\n    \"build\": \"nuxt build\",\n    \"generate\": \"nuxt generate\"\n  },\n  \"dependencies\": {\n    \"nuxt\": \"^3.8.1\"\n  }\n}\n",
        );

        let config = load_config(tmp.path(), BaseConfig::default()).unwrap();

        assert_eq!(
            crate::providers::load_provider_for_test(tmp.path(), &BaseConfig::default(), None)
                .unwrap(),
            "node-static"
        );
        assert_eq!(config.framework, Some(NodeFramework::NuxtV3));
        assert_eq!(config.static_dir.as_deref(), Some(".output/public"));
        assert_eq!(config.build_command.as_deref(), Some("npm run generate"));
    }

    #[test]
    fn test_static_remix_output_can_use_node_static_with_node_dep() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("public")).unwrap();
        write(&tmp.path().join("public/index.html"), "Remix static\n");
        write(
            &tmp.path().join("package.json"),
            "{\n  \"scripts\": {\n    \"build\": \"remix build\",\n    \"start\": \"serve -l 3000 public\"\n  },\n  \"dependencies\": {\n    \"@remix-run/node\": \"^2.2.0\"\n  },\n  \"devDependencies\": {\n    \"@remix-run/dev\": \"^2.2.0\"\n  }\n}\n",
        );

        let config = load_config(tmp.path(), BaseConfig::default()).unwrap();

        assert_eq!(
            crate::providers::load_provider_for_test(tmp.path(), &BaseConfig::default(), None)
                .unwrap(),
            "node-static"
        );
        assert_eq!(config.framework, Some(NodeFramework::RemixV2Classic));
        assert_eq!(config.static_dir.as_deref(), Some("public"));
        assert_eq!(config.build_command.as_deref(), Some("npm run build"));
    }

    #[test]
    fn test_runtime_vite_like_frameworks_are_not_node_static() {
        for dependencies in [
            serde_json::json!({"@react-router/dev": "^7.1.5", "vite": "^5.0.0"}),
            serde_json::json!({"@remix-run/node": "^2.10.0", "@remix-run/dev": "^2.10.0"}),
            serde_json::json!({"@tanstack/react-start": "^1.0.0", "vite": "^5.0.0"}),
            serde_json::json!({"@solidjs/start": "^1.0.0", "vite": "^5.0.0"}),
            serde_json::json!({"@sveltejs/adapter-node": "^5.0.0", "@sveltejs/kit": "^2.16.1"}),
            serde_json::json!({"nitropack": "^2.11.0", "vite": "^5.0.0"}),
            serde_json::json!({"@shopify/hydrogen": "^2026.4.2", "vite": "^7.0.0"}),
        ] {
            let tmp = tempfile::tempdir().unwrap();
            let package_json = serde_json::json!({
                "scripts": {
                    "build": "vite build",
                    "start": "node server.js",
                },
                "dependencies": dependencies,
            });
            write(
                &tmp.path().join("package.json"),
                &format!("{package_json}\n"),
            );
            write(&tmp.path().join("server.js"), "console.log('ok')\n");

            assert!(
                detect(tmp.path(), &BaseConfig::default()).is_none(),
                "{dependencies}"
            );
            assert_ne!(
                crate::providers::load_provider_for_test(tmp.path(), &BaseConfig::default(), None)
                    .unwrap(),
                "node-static",
                "{dependencies}"
            );
        }
    }

    #[test]
    fn test_hydrogen_config_is_not_node_static() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("package.json"),
            "{\n  \"scripts\": {\n    \"build\": \"vite build\",\n    \"start\": \"node server.js\"\n  },\n  \"dependencies\": {\n    \"vite\": \"^7.0.0\"\n  }\n}\n",
        );
        write(
            &tmp.path().join("hydrogen.config.js"),
            "export default {}\n",
        );
        write(&tmp.path().join("server.js"), "console.log('ok')\n");

        assert!(detect(tmp.path(), &BaseConfig::default()).is_none());
        assert_ne!(
            crate::providers::load_provider_for_test(tmp.path(), &BaseConfig::default(), None)
                .unwrap(),
            "node-static"
        );
    }

    #[test]
    fn test_node_static_defaults_to_npm_without_lockfile() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("package.json"),
            "{\n  \"scripts\": {\n    \"build\": \"vitepress build docs\"\n  },\n  \"dependencies\": {\n    \"vitepress\": \"^1.6.4\"\n  }\n}\n",
        );

        let config = load_config(tmp.path(), BaseConfig::default()).unwrap();

        assert_eq!(config.package_manager, Some(PackageManager::Npm));
        assert_eq!(config.build_command.as_deref(), Some("npm run build"));
    }

    #[test]
    fn test_node_static_uses_pnpm_when_lockfile_is_present() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("package.json"),
            "{\n  \"scripts\": {\n    \"build\": \"vitepress build docs\"\n  },\n  \"dependencies\": {\n    \"vitepress\": \"^1.6.4\"\n  }\n}\n",
        );
        write(
            &tmp.path().join("pnpm-lock.yaml"),
            "lockfileVersion: '9.0'\n",
        );

        let config = load_config(tmp.path(), BaseConfig::default()).unwrap();

        assert_eq!(config.package_manager, Some(PackageManager::Pnpm));
        assert_eq!(config.build_command.as_deref(), Some("pnpm run build"));
    }

    #[test]
    fn test_node_static_script_commands_prefers_build_over_fallbacks() {
        let package_json = match serde_json::json!({
            "scripts": {
                "build": "vite build",
                "generate": "vite generate",
                "docs:build": "vitepress build docs",
            },
        }) {
            Value::Object(map) => map,
            _ => unreachable!(),
        };

        // NodeStaticProvider._script_commands defaults to
        // preferred=SCRIPT_BUILD_COMMAND.
        assert_eq!(
            static_script_commands(Some(&package_json), SCRIPT_BUILD_COMMAND),
            vec!["vite build"]
        );
    }

    #[test]
    fn test_new_static_builder_commands_are_detected() {
        use NodeFramework::*;
        let cases: &[(&str, &[NodeFramework])] = &[
            ("npx @11ty/eleventy", &[Eleventy]),
            ("vitepress build docs", &[Vitepress]),
            ("vuepress build docs", &[Vuepress]),
            ("hexo g", &[Hexo]),
            ("metalsmith", &[Metalsmith]),
            ("grunt assemble", &[Assemble]),
            ("harp compile src www", &[Harp]),
            ("ng build", &[IonicAngular, Angular]),
            ("brunch build --production", &[Brunch]),
            ("react-scripts build", &[IonicReact, CreateReactApp]),
            ("ember build --environment=production", &[Ember]),
            ("parcel build src/index.html", &[Parcel]),
            ("polymer build", &[Polymer]),
            ("preact build", &[Preact]),
            ("stencil build", &[Stencil]),
            ("svelte-kit build", &[Sveltekit]),
            ("umi build", &[Umijs]),
            ("vue-cli-service build", &[Vue]),
            ("nuxt generate", &[NuxtOld]),
            ("sanity build", &[Sanity]),
            ("storybook build", &[Storybook]),
        ];

        for (command, frameworks) in cases {
            assert_eq!(
                NodeFramework::detect_from_command(command),
                *frameworks,
                "{command}"
            );
        }
    }

    #[test]
    fn test_node_framework_static_capability_is_explicit() {
        assert!(NodeFramework::Next.can_be_static());
        assert!(NodeFramework::Eleventy.can_be_static());
        assert!(!NodeFramework::Express.can_be_static());
    }
}
