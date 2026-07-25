//! Node provider implementation.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::LazyLock;

use anyhow::{Context, Result};
use indexmap::IndexMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::operation::OperationContext;
use crate::providers::base::{
    env_bool, env_enum, env_json, env_str, BaseConfig, DetectResult, HasBase,
};
use crate::providers::install_context::{discover_js_install_context, read_json_object};

pub(crate) use crate::providers::install_context::JsonMap;

const NEXT_BUNDLE_VERSION: &str = "1.0.0";

// ---------------------------------------------------------------------------
// PackageManager

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl PackageManager {
    /// Python enum definition order (`for manager in PackageManager`).
    pub const ALL: [PackageManager; 4] = [
        PackageManager::Npm,
        PackageManager::Pnpm,
        PackageManager::Yarn,
        PackageManager::Bun,
    ];

    pub fn value(self) -> &'static str {
        match self {
            PackageManager::Npm => "npm",
            PackageManager::Pnpm => "pnpm",
            PackageManager::Yarn => "yarn",
            PackageManager::Bun => "bun",
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "npm" => Some(PackageManager::Npm),
            "pnpm" => Some(PackageManager::Pnpm),
            "yarn" => Some(PackageManager::Yarn),
            "bun" => Some(PackageManager::Bun),
            _ => None,
        }
    }

    fn lockfiles(self) -> &'static [&'static str] {
        match self {
            PackageManager::Npm => &["package-lock.json"],
            PackageManager::Pnpm => &["pnpm-lock.yaml"],
            PackageManager::Yarn => &["yarn.lock"],
            PackageManager::Bun => &["bun.lock", "bun.lockb"],
        }
    }

    pub(crate) fn has_lockfile(self, path: &Path) -> bool {
        self.lockfiles()
            .iter()
            .any(|lockfile| path.join(lockfile).exists())
    }

    pub fn run_command(self, command: &str) -> String {
        format!("{} run {command}", self.value())
    }

    pub fn run_execute_command(self, command: &str) -> String {
        let runner = match self {
            PackageManager::Npm => "npx",
            PackageManager::Pnpm => "pnpx",
            PackageManager::Yarn => "ypx",
            PackageManager::Bun => "bunx",
        };
        format!("{runner} {command}")
    }

    pub fn dlx_command(self, command: &str) -> String {
        let runner = match self {
            PackageManager::Npm => "npx -y",
            PackageManager::Pnpm => "pnpm dlx",
            PackageManager::Yarn => "yarn dlx",
            PackageManager::Bun => "bunx",
        };
        format!("{runner} {command}")
    }
}

// ---------------------------------------------------------------------------
// NodeFramework

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeFramework {
    #[serde(rename = "angular")]
    Angular,
    #[serde(rename = "next")]
    Next,
    #[serde(rename = "astro")]
    Astro,
    #[serde(rename = "brunch")]
    Brunch,
    #[serde(rename = "create-react-app")]
    CreateReactApp,
    #[serde(rename = "vite")]
    Vite,
    #[serde(rename = "gatsby")]
    Gatsby,
    #[serde(rename = "eleventy")]
    Eleventy,
    #[serde(rename = "ember")]
    Ember,
    #[serde(rename = "vitepress")]
    Vitepress,
    #[serde(rename = "vuepress")]
    Vuepress,
    #[serde(rename = "hexo")]
    Hexo,
    #[serde(rename = "ionic-angular")]
    IonicAngular,
    #[serde(rename = "ionic-react")]
    IonicReact,
    #[serde(rename = "metalsmith")]
    Metalsmith,
    #[serde(rename = "assemble")]
    Assemble,
    #[serde(rename = "harp")]
    Harp,
    #[serde(rename = "parcel")]
    Parcel,
    #[serde(rename = "polymer")]
    Polymer,
    #[serde(rename = "preact")]
    Preact,
    #[serde(rename = "stencil")]
    Stencil,
    #[serde(rename = "docusaurus-old")]
    DocusaurusOld,
    #[serde(rename = "docusaurus")]
    Docusaurus,
    #[serde(rename = "svelte")]
    Svelte,
    #[serde(rename = "sveltekit")]
    Sveltekit,
    #[serde(rename = "umijs")]
    Umijs,
    #[serde(rename = "vue")]
    Vue,
    #[serde(rename = "remix")]
    Remix,
    #[serde(rename = "nuxt")]
    NuxtOld,
    #[serde(rename = "nuxt3")]
    NuxtV3,
    #[serde(rename = "remix-old")]
    RemixOld,
    #[serde(rename = "remix-v2")]
    RemixV2,
    #[serde(rename = "remix-v2-classic")]
    RemixV2Classic,
    #[serde(rename = "react-router")]
    ReactRouter,
    #[serde(rename = "nitro")]
    Nitro,
    #[serde(rename = "solidstart")]
    Solidstart,
    #[serde(rename = "tanstack-start")]
    TanstackStart,
    #[serde(rename = "hydrogen")]
    Hydrogen,
    #[serde(rename = "hono")]
    Hono,
    #[serde(rename = "express")]
    Express,
    #[serde(rename = "h3")]
    H3,
    #[serde(rename = "koa")]
    Koa,
    #[serde(rename = "nestjs")]
    Nestjs,
    #[serde(rename = "elysia")]
    Elysia,
    #[serde(rename = "fastify")]
    Fastify,
    #[serde(rename = "xmcp")]
    Xmcp,
    #[serde(rename = "mastra")]
    Mastra,
    #[serde(rename = "sanity")]
    Sanity,
    #[serde(rename = "sanity-v3")]
    SanityV3,
    #[serde(rename = "storybook")]
    Storybook,
}

impl NodeFramework {
    /// Python enum definition order (`for framework in NodeFramework`).
    pub const ALL: [NodeFramework; 50] = [
        NodeFramework::Angular,
        NodeFramework::Next,
        NodeFramework::Astro,
        NodeFramework::Brunch,
        NodeFramework::CreateReactApp,
        NodeFramework::Vite,
        NodeFramework::Gatsby,
        NodeFramework::Eleventy,
        NodeFramework::Ember,
        NodeFramework::Vitepress,
        NodeFramework::Vuepress,
        NodeFramework::Hexo,
        NodeFramework::IonicAngular,
        NodeFramework::IonicReact,
        NodeFramework::Metalsmith,
        NodeFramework::Assemble,
        NodeFramework::Harp,
        NodeFramework::Parcel,
        NodeFramework::Polymer,
        NodeFramework::Preact,
        NodeFramework::Stencil,
        NodeFramework::DocusaurusOld,
        NodeFramework::Docusaurus,
        NodeFramework::Svelte,
        NodeFramework::Sveltekit,
        NodeFramework::Umijs,
        NodeFramework::Vue,
        NodeFramework::Remix,
        NodeFramework::NuxtOld,
        NodeFramework::NuxtV3,
        NodeFramework::RemixOld,
        NodeFramework::RemixV2,
        NodeFramework::RemixV2Classic,
        NodeFramework::ReactRouter,
        NodeFramework::Nitro,
        NodeFramework::Solidstart,
        NodeFramework::TanstackStart,
        NodeFramework::Hydrogen,
        NodeFramework::Hono,
        NodeFramework::Express,
        NodeFramework::H3,
        NodeFramework::Koa,
        NodeFramework::Nestjs,
        NodeFramework::Elysia,
        NodeFramework::Fastify,
        NodeFramework::Xmcp,
        NodeFramework::Mastra,
        NodeFramework::Sanity,
        NodeFramework::SanityV3,
        NodeFramework::Storybook,
    ];

    pub fn can_be_static(self) -> bool {
        matches!(
            self,
            NodeFramework::Angular
                | NodeFramework::Astro
                | NodeFramework::Brunch
                | NodeFramework::CreateReactApp
                | NodeFramework::Ember
                | NodeFramework::Vite
                | NodeFramework::Next
                | NodeFramework::Gatsby
                | NodeFramework::Eleventy
                | NodeFramework::Vitepress
                | NodeFramework::Vuepress
                | NodeFramework::Hexo
                | NodeFramework::IonicAngular
                | NodeFramework::IonicReact
                | NodeFramework::Metalsmith
                | NodeFramework::Assemble
                | NodeFramework::Harp
                | NodeFramework::Parcel
                | NodeFramework::Polymer
                | NodeFramework::Preact
                | NodeFramework::Stencil
                | NodeFramework::DocusaurusOld
                | NodeFramework::Docusaurus
                | NodeFramework::Svelte
                | NodeFramework::Sveltekit
                | NodeFramework::Umijs
                | NodeFramework::Vue
                | NodeFramework::Remix
                | NodeFramework::NuxtOld
                | NodeFramework::NuxtV3
                | NodeFramework::RemixOld
                | NodeFramework::RemixV2
                | NodeFramework::RemixV2Classic
                | NodeFramework::Sanity
                | NodeFramework::SanityV3
                | NodeFramework::Storybook
        )
    }

    pub fn is_pure_static(self) -> bool {
        matches!(
            self,
            NodeFramework::Angular
                | NodeFramework::Brunch
                | NodeFramework::CreateReactApp
                | NodeFramework::Eleventy
                | NodeFramework::Ember
                | NodeFramework::Vitepress
                | NodeFramework::Vuepress
                | NodeFramework::Assemble
                | NodeFramework::Harp
                | NodeFramework::Hexo
                | NodeFramework::IonicAngular
                | NodeFramework::IonicReact
                | NodeFramework::Metalsmith
                | NodeFramework::Parcel
                | NodeFramework::Polymer
                | NodeFramework::Preact
                | NodeFramework::Stencil
                | NodeFramework::Docusaurus
                | NodeFramework::DocusaurusOld
                | NodeFramework::Svelte
                | NodeFramework::Sveltekit
                | NodeFramework::Umijs
                | NodeFramework::Vite
                | NodeFramework::Vue
                | NodeFramework::Sanity
                | NodeFramework::SanityV3
                | NodeFramework::Storybook
        )
    }

    /// Python raises for frameworks without a static output dir; callers
    /// only reach this for static-capable frameworks.
    pub fn get_static_output_dir(self) -> Option<&'static str> {
        Some(match self {
            NodeFramework::Angular => "dist",
            NodeFramework::Next => "out",
            NodeFramework::Eleventy => "_site",
            NodeFramework::NuxtV3 => ".output/public",
            NodeFramework::Astro => "dist",
            NodeFramework::Brunch => "public",
            NodeFramework::CreateReactApp => "build",
            NodeFramework::Ember => "dist",
            NodeFramework::Vite => "dist",
            NodeFramework::NuxtOld => "dist",
            NodeFramework::Gatsby => "public",
            NodeFramework::Hexo => "public",
            NodeFramework::IonicAngular => "www",
            NodeFramework::IonicReact => "dist",
            NodeFramework::Vitepress => "docs/.vitepress/dist",
            NodeFramework::Vuepress => "docs/.vuepress/dist",
            NodeFramework::RemixOld => "build/client",
            NodeFramework::Remix => "build/client",
            NodeFramework::RemixV2 => "build/client",
            NodeFramework::RemixV2Classic => "public",
            NodeFramework::Assemble => "dist",
            NodeFramework::Harp => "www",
            NodeFramework::Parcel => "dist",
            NodeFramework::Polymer => "build/default",
            NodeFramework::Preact => "build",
            NodeFramework::Stencil => "www",
            NodeFramework::Docusaurus => "build",
            NodeFramework::DocusaurusOld => "build",
            NodeFramework::Svelte => "build",
            NodeFramework::Sveltekit => "build",
            NodeFramework::Umijs => "dist",
            NodeFramework::Vue => "dist",
            NodeFramework::Metalsmith => "build",
            NodeFramework::Sanity => "dist",
            NodeFramework::SanityV3 => "dist",
            NodeFramework::Storybook => "storybook-static",
            _ => return None,
        })
    }

    pub fn detect_from_command(build_command: &str) -> Vec<NodeFramework> {
        use NodeFramework::*;
        const COMMANDS: &[(&str, &[NodeFramework])] = &[
            ("ng", &[IonicAngular, Angular]),
            ("gatsby", &[Gatsby]),
            ("astro", &[Astro]),
            ("@11ty/eleventy", &[Eleventy]),
            ("eleventy", &[Eleventy]),
            ("brunch", &[Brunch]),
            ("react-scripts", &[IonicReact, CreateReactApp]),
            ("remix-ssg", &[RemixOld]),
            ("remix", &[RemixV2Classic, RemixV2]),
            ("ember", &[Ember]),
            ("vite", &[Vite]),
            ("vitepress", &[Vitepress]),
            ("vuepress", &[Vuepress]),
            ("hexo", &[Hexo]),
            ("metalsmith", &[Metalsmith]),
            ("harp", &[Harp]),
            ("parcel", &[Parcel]),
            ("polymer", &[Polymer]),
            ("preact", &[Preact]),
            ("stencil", &[Stencil]),
            ("docusaurus", &[Docusaurus, DocusaurusOld]),
            ("next", &[Next]),
            ("nuxi", &[NuxtV3]),
            ("nuxt", &[NuxtOld]),
            ("svelte-kit", &[Sveltekit]),
            ("umi", &[Umijs]),
            ("vue-cli-service", &[Vue]),
            ("sanity", &[Sanity]),
            ("storybook", &[Storybook]),
        ];

        let tokens = split_command(build_command);
        for (index, token) in tokens.iter().enumerate() {
            if token == "grunt" && tokens[index + 1..].iter().any(|t| t == "assemble") {
                return vec![Assemble];
            }
            if let Some((_, frameworks)) = COMMANDS.iter().find(|(cmd, _)| cmd == token) {
                return frameworks.to_vec();
            }
        }
        Vec::new()
    }

    /// Python raises for frameworks without a static build command;
    /// callers only reach this for static-capable frameworks.
    pub fn build_static_command(self) -> Option<&'static str> {
        Some(match self {
            NodeFramework::Angular => "ng build",
            NodeFramework::Gatsby => "gatsby build",
            NodeFramework::Eleventy => "@11ty/eleventy",
            NodeFramework::Vitepress => "vitepress build docs",
            NodeFramework::Vuepress => "vuepress build docs",
            NodeFramework::Hexo => "hexo generate",
            NodeFramework::Metalsmith => "metalsmith build",
            NodeFramework::Assemble => "grunt assemble",
            NodeFramework::Harp => "harp compile . www",
            NodeFramework::Brunch => "brunch build --production",
            NodeFramework::CreateReactApp => "react-scripts build",
            NodeFramework::Ember => "ember build",
            NodeFramework::IonicAngular => "ng build",
            NodeFramework::IonicReact => "vite build",
            NodeFramework::Parcel => "parcel build",
            NodeFramework::Polymer => "polymer build",
            NodeFramework::Preact => "preact build",
            NodeFramework::Stencil => "stencil build",
            NodeFramework::Astro => "astro build",
            NodeFramework::RemixOld => "remix-ssg build",
            NodeFramework::RemixV2 => "vite build",
            NodeFramework::RemixV2Classic => "remix build",
            NodeFramework::Docusaurus => "docusaurus build",
            NodeFramework::DocusaurusOld => "docusaurus build",
            NodeFramework::Svelte => "svelte-kit build",
            NodeFramework::Sveltekit => "svelte-kit build",
            NodeFramework::Umijs => "umi build",
            NodeFramework::Vite => "vite build",
            NodeFramework::Vue => "vue-cli-service build",
            NodeFramework::Next => "next export",
            NodeFramework::NuxtV3 => "nuxi generate",
            NodeFramework::NuxtOld => "nuxt generate",
            NodeFramework::Remix => "remix build",
            NodeFramework::Sanity => "sanity build",
            NodeFramework::SanityV3 => "sanity build",
            NodeFramework::Storybook => "storybook build",
            _ => return None,
        })
    }

    pub fn bundle_build_command(
        self,
        package_manager: PackageManager,
        build_command: &str,
    ) -> Result<String> {
        if self == NodeFramework::Next {
            let quoted_command =
                shlex::try_quote(build_command).context("Cannot shell-quote Node build command")?;
            return Ok(package_manager.dlx_command(&format!(
                "next-bundle@{NEXT_BUNDLE_VERSION} --build-command {quoted_command}"
            )));
        }
        Ok(build_command.to_owned())
    }

    pub fn start_command(self) -> Option<&'static str> {
        match self {
            NodeFramework::Next => Some("node server.mjs"),
            NodeFramework::Nitro => Some("node .output/server/index.mjs"),
            _ => None,
        }
    }

    fn from_value(value: &str) -> Option<Self> {
        serde_json::from_value(Value::String(value.to_owned())).ok()
    }
}

// ---------------------------------------------------------------------------
// NodeConfig

/// Node-specific config fields shared by Node, Node Static, and Laravel.
///
/// `F` preserves Laravel's Python-compatible `Any` framework field while
/// ordinary Node configs retain the typed `NodeFramework` enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NodeConfigFields<F = NodeFramework> {
    pub use_edgejs: Option<bool>,
    pub precompile_edgejs: Option<bool>,
    pub package_manager: Option<PackageManager>,
    pub framework: Option<F>,
    pub extra_dependencies: BTreeSet<String>,
    pub build_command: Option<String>,
    pub node_version: Option<String>,
    pub npm_version: Option<String>,
    pub pnpm_version: Option<String>,
    pub yarn_version: Option<String>,
    pub bun_version: Option<String>,
    pub optimize_node_dependencies: Option<bool>,
    /// Optimize node_modules size further when targeting Edge by removing
    /// executable native binaries that cannot run there anyway.
    pub remove_native_binaries: Option<bool>,
    pub install_requires_all_files: bool,
    // Derived facts for the Starlark provider.
    pub install_inputs: Option<Vec<String>>,
    pub package_name: Option<String>,
}

impl<F> Default for NodeConfigFields<F> {
    fn default() -> Self {
        Self {
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
        }
    }
}

impl NodeConfigFields<NodeFramework> {
    fn from_env(operation: &OperationContext) -> Result<Self> {
        Ok(Self {
            use_edgejs: env_bool(operation, "use_edgejs")?.or(Some(false)),
            precompile_edgejs: env_bool(operation, "precompile_edgejs")?,
            package_manager: env_enum(
                operation,
                "package_manager",
                "a supported package manager",
                PackageManager::from_name,
            )?,
            framework: env_enum(
                operation,
                "framework",
                "a supported Node framework",
                NodeFramework::from_value,
            )?,
            extra_dependencies: env_json(operation, "extra_dependencies")?.unwrap_or_default(),
            build_command: env_str(operation, "build_command"),
            node_version: env_str(operation, "node_version").or_else(|| Some("24".to_owned())),
            npm_version: env_str(operation, "npm_version"),
            pnpm_version: env_str(operation, "pnpm_version"),
            yarn_version: env_str(operation, "yarn_version"),
            bun_version: env_str(operation, "bun_version"),
            optimize_node_dependencies: env_bool(operation, "optimize_node_dependencies")?
                .or(Some(true)),
            remove_native_binaries: env_bool(operation, "remove_native_binaries")?.or(Some(false)),
            install_requires_all_files: env_bool(operation, "install_requires_all_files")?
                .unwrap_or(false),
            install_inputs: env_json(operation, "install_inputs")?,
            package_name: env_str(operation, "package_name"),
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NodeConfig {
    #[serde(flatten)]
    pub base: BaseConfig,
    #[serde(flatten)]
    pub node: NodeConfigFields,
}

impl NodeConfig {
    /// pydantic-settings construction: `NodeConfig(**base.model_dump())`
    /// reads `ANYBUILD_<FIELD>` for every non-base field.
    pub(crate) fn from_env(base: BaseConfig, operation: &OperationContext) -> Result<Self> {
        Ok(Self {
            base,
            node: NodeConfigFields::from_env(operation)?,
        })
    }
}

impl std::ops::Deref for NodeConfig {
    type Target = NodeConfigFields;

    fn deref(&self) -> &Self::Target {
        &self.node
    }
}

impl std::ops::DerefMut for NodeConfig {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.node
    }
}

impl HasBase for NodeConfig {
    fn base(&self) -> &BaseConfig {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseConfig {
        &mut self.base
    }
}

/// Python truthiness for optional command strings ("" is falsy).
pub(crate) fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|s| !s.is_empty())
}

// ---------------------------------------------------------------------------
// Detection

pub(crate) const FRAMEWORK_DEPENDENCIES: &[&str] = &[
    "next",
    "astro",
    "@react-router/dev",
    "@react-router/node",
    "@react-router/serve",
    "@remix-run/dev",
    "@remix-run/node",
    "@remix-run/react",
    "@remix-run/server-runtime",
    "@sveltejs/kit",
    "@sveltejs/adapter-node",
    "nitropack",
    "nitro",
    "@solidjs/start",
    "solid-start",
    "solid-js",
    "@tanstack/react-start",
    "@tanstack/solid-start",
    "@shopify/hydrogen",
    "@shopify/remix-oxygen",
    "@nestjs/common",
    "@nestjs/core",
    "@nestjs/platform-express",
    "@nestjs/platform-fastify",
    "hono",
    "@hono/node-server",
    "express",
    "h3",
    "koa",
    "elysia",
    "@elysia/node",
    "fastify",
    "xmcp",
    "mastra",
    "@mastra/core",
];

const HYDROGEN_CONFIG_FILES: &[&str] = &["hydrogen.config.js", "hydrogen.config.ts"];

const COMMON_ENTRY_FILES: &[&str] = &[
    "server.js",
    "app.js",
    "index.js",
    "src/server.js",
    "src/index.js",
];

const NODE_BUILTIN_MODULES: &[&str] = &[
    "assert",
    "buffer",
    "child_process",
    "cluster",
    "crypto",
    "dgram",
    "dns",
    "events",
    "fs",
    "http",
    "http2",
    "https",
    "module",
    "net",
    "os",
    "path",
    "perf_hooks",
    "process",
    "querystring",
    "readline",
    "stream",
    "timers",
    "tls",
    "tty",
    "url",
    "util",
    "v8",
    "vm",
    "worker_threads",
    "zlib",
];

static NODE_ENTRY_MARKER_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\bmodule\.exports\b|\bexports\.|\b__dirname\b|\b__filename\b|\bprocess\.(?:env|argv|exit|cwd|platform|version)\b|\b[A-Za-z_$][\w$]*\.listen\s*\(",
    )
    .expect("valid regex")
});

static NODE_SHEBANG_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bnode(?:js)?\b").expect("valid regex"));

static NODE_MODULE_REF_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    let modules = NODE_BUILTIN_MODULES
        .iter()
        .map(|module| regex::escape(module))
        .collect::<Vec<_>>()
        .join("|");
    let module_ref = format!(r#"(?:node:)?(?:{modules})(?:/[^'"\s)]*)?"#);
    [
        format!(r#"\brequire\s*\(\s*['"]{module_ref}['"]\s*\)"#),
        format!(r#"\bimport\s*\(\s*['"]{module_ref}['"]\s*\)"#),
        format!(r#"\bfrom\s*['"]{module_ref}['"]"#),
        format!(r#"\bimport\s*['"]{module_ref}['"]"#),
    ]
    .iter()
    .map(|pattern| Regex::new(pattern).expect("valid regex"))
    .collect()
});

const START_PROGRAMS: &[&str] = &[
    "edge", "node", "npm", "npx", "pnpm", "pnpx", "yarn", "ypx", "bun", "bunx",
];

const INSTALL_COMMANDS: &[&str] = &[
    "npm install",
    "npm ci",
    "npm i",
    "pnpm install",
    "pnpm ci",
    "pnpm i",
    "yarn install",
    "yarn ci",
    "yarn i",
    "bun install",
    "bun ci",
    "bun i",
];

pub fn detect(
    path: &Path,
    base: &BaseConfig,
    _operation: &OperationContext,
) -> Option<DetectResult> {
    if let Some(start) = non_empty(&base.commands.start) {
        if is_node_command(start) {
            return Some(DetectResult {
                name: "node",
                score: 35,
            });
        }
    }

    let package_json = parse_package_json(path);
    let has_package_start = package_scripts(package_json.as_ref())
        .get("start")
        .is_some_and(|start| !start.trim().is_empty());
    let package_score = if has_package_start { 30 } else { 10 };

    if let Some(install) = non_empty(&base.commands.install) {
        if INSTALL_COMMANDS.contains(&install) {
            return Some(DetectResult {
                name: "node",
                score: package_score,
            });
        }
    }

    let found_deps = check_package_json_deps(package_json.as_ref(), FRAMEWORK_DEPENDENCIES);
    if detect_framework(package_json.as_ref(), &found_deps, Some(path)).is_some() {
        return Some(DetectResult {
            name: "node",
            score: if has_package_start { 45 } else { 10 },
        });
    }

    if path.join("package.json").is_file() {
        return Some(DetectResult {
            name: "node",
            score: package_score,
        });
    }

    if common_entry_file(path, true).is_some() {
        return Some(DetectResult {
            name: "node",
            score: 30,
        });
    }

    None
}

fn common_entry_file(path: &Path, require_node_evidence: bool) -> Option<&'static str> {
    for entry_file in COMMON_ENTRY_FILES {
        let entry_path = path.join(entry_file);
        if !entry_path.is_file() {
            continue;
        }
        if !require_node_evidence {
            return Some(entry_file);
        }
        if looks_like_node_entry(&entry_path) {
            return Some(entry_file);
        }
    }
    None
}

fn looks_like_node_entry(entry_path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(entry_path) else {
        return false;
    };
    let source = String::from_utf8_lossy(&bytes);

    let first_line = source.lines().next().unwrap_or("");
    if first_line.starts_with("#!") && NODE_SHEBANG_PATTERN.is_match(first_line) {
        return true;
    }

    if NODE_ENTRY_MARKER_PATTERN.is_match(&source) {
        return true;
    }

    NODE_MODULE_REF_PATTERNS
        .iter()
        .any(|pattern| pattern.is_match(&source))
}

pub fn detect_package_manager(path: &Path) -> PackageManager {
    let package_json = parse_package_json(path);
    if let Some(field) = package_json
        .as_ref()
        .and_then(|pj| pj.get("packageManager"))
        .and_then(Value::as_str)
    {
        let name = field.split('@').next().unwrap_or("").to_lowercase();
        if let Some(manager) = PackageManager::from_name(&name) {
            return manager;
        }
    }

    if path.join("pnpm-workspace.yaml").exists() {
        return PackageManager::Pnpm;
    }
    for manager in PackageManager::ALL {
        if manager.has_lockfile(path) {
            return manager;
        }
    }
    PackageManager::Npm
}

// ---------------------------------------------------------------------------
// package.json helpers

pub(crate) fn parse_package_json(path: &Path) -> Option<JsonMap> {
    read_json_object(&path.join("package.json"))
}

/// str→str script entries, in package.json order.
pub(crate) fn package_scripts(package_json: Option<&JsonMap>) -> IndexMap<&str, &str> {
    let mut scripts = IndexMap::new();
    let Some(Value::Object(raw)) = package_json.and_then(|pj| pj.get("scripts")) else {
        return scripts;
    };
    for (name, command) in raw {
        if let Some(command) = command.as_str() {
            scripts.insert(name.as_str(), command);
        }
    }
    scripts
}

pub(crate) fn package_json_name(package_json: Option<&JsonMap>) -> Option<String> {
    let name = package_json?.get("name")?.as_str()?;
    if name.is_empty() {
        None
    } else {
        Some(name.to_owned())
    }
}

pub(crate) fn check_package_json_deps(
    package_json: Option<&JsonMap>,
    deps: &[&'static str],
) -> BTreeSet<&'static str> {
    let mut found = BTreeSet::new();
    let Some(package_json) = package_json else {
        return found;
    };
    for section in ["dependencies", "devDependencies", "peerDependencies"] {
        let Some(Value::Object(dep_section)) = package_json.get(section) else {
            continue;
        };
        for dep in deps {
            if dep_section.contains_key(*dep) {
                found.insert(*dep);
            }
        }
    }
    found
}

/// `_script_commands`: preferred script names first (exact), otherwise
/// scripts whose name starts with any preferred name, in file order.
pub(crate) fn script_commands<'a>(
    package_json: Option<&'a JsonMap>,
    preferred: &[&str],
) -> Vec<&'a str> {
    let scripts = package_scripts(package_json);
    let mut commands: Vec<&str> = preferred
        .iter()
        .filter_map(|name| scripts.get(name).copied())
        .collect();
    if commands.is_empty() {
        commands.extend(
            scripts
                .iter()
                .filter(|(name, _)| preferred.iter().any(|p| name.starts_with(p)))
                .map(|(_, command)| *command),
        );
    }
    commands
}

pub(crate) fn is_package_manager_build_command(command: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "npm run ",
        "pnpm run ",
        "pnpm ",
        "yarn run ",
        "yarn ",
        "bun run ",
        "bun ",
    ];
    PREFIXES.iter().any(|prefix| command.starts_with(prefix))
}

fn is_node_command(command: &str) -> bool {
    let parts = split_command(command);
    parts
        .first()
        .is_some_and(|part| START_PROGRAMS.contains(&part.as_str()))
}

pub(crate) fn has_hydrogen_config(path: Option<&Path>) -> bool {
    let Some(path) = path else {
        return false;
    };
    HYDROGEN_CONFIG_FILES
        .iter()
        .any(|file| path.join(file).is_file())
}

// ---------------------------------------------------------------------------
// Framework detection

pub(crate) fn detect_framework(
    package_json: Option<&JsonMap>,
    found_deps: &BTreeSet<&'static str>,
    path: Option<&Path>,
) -> Option<NodeFramework> {
    let any = |deps: &[&str]| deps.iter().any(|dep| found_deps.contains(dep));

    if found_deps.contains("next") {
        return Some(NodeFramework::Next);
    }
    if found_deps.contains("astro") {
        return Some(NodeFramework::Astro);
    }
    if any(&["@shopify/hydrogen", "@shopify/remix-oxygen"]) || has_hydrogen_config(path) {
        return Some(NodeFramework::Hydrogen);
    }
    if any(&[
        "@react-router/dev",
        "@react-router/node",
        "@react-router/serve",
    ]) {
        return Some(NodeFramework::ReactRouter);
    }
    if any(&[
        "@remix-run/dev",
        "@remix-run/node",
        "@remix-run/react",
        "@remix-run/server-runtime",
    ]) {
        return Some(NodeFramework::Remix);
    }
    if found_deps.contains("@sveltejs/kit") {
        return Some(NodeFramework::Sveltekit);
    }
    if any(&["nitropack", "nitro"]) {
        return Some(NodeFramework::Nitro);
    }
    if any(&["@solidjs/start", "solid-start"]) {
        return Some(NodeFramework::Solidstart);
    }
    if any(&["@tanstack/react-start", "@tanstack/solid-start"]) {
        return Some(NodeFramework::TanstackStart);
    }
    if any(&[
        "@nestjs/common",
        "@nestjs/core",
        "@nestjs/platform-express",
        "@nestjs/platform-fastify",
    ]) {
        return Some(NodeFramework::Nestjs);
    }
    if any(&["hono", "@hono/node-server"]) {
        return Some(NodeFramework::Hono);
    }
    if found_deps.contains("express") {
        return Some(NodeFramework::Express);
    }
    if found_deps.contains("h3") {
        return Some(NodeFramework::H3);
    }
    if found_deps.contains("koa") {
        return Some(NodeFramework::Koa);
    }
    if any(&["elysia", "@elysia/node"]) {
        return Some(NodeFramework::Elysia);
    }
    if found_deps.contains("fastify") {
        return Some(NodeFramework::Fastify);
    }
    if found_deps.contains("xmcp") {
        return Some(NodeFramework::Xmcp);
    }
    if any(&["mastra", "@mastra/core"]) {
        return Some(NodeFramework::Mastra);
    }

    for command in script_commands(package_json, &["build"]) {
        if split_command(command).iter().any(|token| token == "next") {
            return Some(NodeFramework::Next);
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Dependency version helpers (semantic_version.NpmSpec subset)

pub(crate) fn dependency_version(package_json: Option<&JsonMap>, dep: &str) -> Option<String> {
    let package_json = package_json?;
    for section in ["dependencies", "devDependencies", "peerDependencies"] {
        let Some(Value::Object(dep_section)) = package_json.get(section) else {
            continue;
        };
        if let Some(version) = dep_section.get(dep).and_then(Value::as_str) {
            return Some(version.to_owned());
        }
    }
    None
}

/// `has_dependency(pj, dep, version)`. With a version, Python builds
/// `Version(version)`, which rejects partial strings ("0"/"1"/"2" — the
/// only call sites); the exception falls through to the next section,
/// ending in False.
pub(crate) fn has_dependency(
    package_json: Option<&JsonMap>,
    dep: &str,
    version: Option<&str>,
) -> bool {
    let Some(package_json) = package_json else {
        return false;
    };
    for section in ["dependencies", "devDependencies", "peerDependencies"] {
        let Some(Value::Object(dep_section)) = package_json.get(section) else {
            continue;
        };
        if let Some(spec) = dep_section.get(dep) {
            match version {
                None => return true,
                Some(version) => {
                    if let (Some(target), Some(spec)) =
                        (parse_strict_version(version), spec.as_str())
                    {
                        if let Some(matched) = version_matches_npm_spec(target, spec) {
                            return matched;
                        }
                    }
                    // Exception path: keep scanning the remaining sections.
                }
            }
        }
    }
    false
}

pub(crate) fn has_dependency_major(package_json: Option<&JsonMap>, dep: &str, major: u64) -> bool {
    let Some(version) = dependency_version(package_json, dep) else {
        return false;
    };
    if let Some(matched) = version_matches_npm_spec((major, 999, 999), &version) {
        return matched;
    }
    let normalized = version.trim_start_matches(|c| "^~>=< ".contains(c));
    normalized == major.to_string() || normalized.starts_with(&format!("{major}."))
}

/// `semantic_version.Version`: full X.Y.Z release triples only.
fn parse_strict_version(version: &str) -> Option<(u64, u64, u64)> {
    let release = version.split(['-', '+']).next().unwrap_or("");
    let parts: Vec<&str> = release.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    Some((
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ))
}

#[derive(Clone, Copy)]
struct PartialVersion {
    major: Option<u64>,
    minor: Option<u64>,
    patch: Option<u64>,
}

impl PartialVersion {
    fn floor(self) -> (u64, u64, u64) {
        (
            self.major.unwrap_or(0),
            self.minor.unwrap_or(0),
            self.patch.unwrap_or(0),
        )
    }
}

fn parse_partial(token: &str) -> Option<PartialVersion> {
    let token = token.trim().trim_start_matches('v');
    let token = token.split(['-', '+']).next().unwrap_or("");
    if token.is_empty() {
        return None;
    }
    let mut components: Vec<Option<u64>> = Vec::new();
    for part in token.split('.') {
        if components.len() >= 3 {
            return None;
        }
        match part {
            "x" | "X" | "*" => components.push(None),
            "" => return None,
            part => components.push(Some(part.parse().ok()?)),
        }
    }
    Some(PartialVersion {
        major: components.first().copied().flatten(),
        minor: components.get(1).copied().flatten(),
        patch: components.get(2).copied().flatten(),
    })
}

/// npm-range evaluation; `None` mirrors an NpmSpec parse failure (the
/// callers' `except Exception` fallback path).
fn version_matches_npm_spec(version: (u64, u64, u64), spec: &str) -> Option<bool> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Some(true); // NpmSpec("") matches everything.
    }
    let mut any = false;
    for clause in spec.split("||") {
        any = clause_matches(version, clause.trim())? || any;
    }
    Some(any)
}

fn clause_matches(version: (u64, u64, u64), clause: &str) -> Option<bool> {
    if clause.is_empty() {
        return Some(true);
    }
    let tokens: Vec<&str> = clause.split_whitespace().collect();
    if let Some(pos) = tokens.iter().position(|t| *t == "-") {
        if pos != 1 || tokens.len() != 3 {
            return None;
        }
        let low = parse_partial(tokens[0])?;
        let high = parse_partial(tokens[2])?;
        let lower_ok = version >= low.floor();
        let upper_ok = match (high.major, high.minor, high.patch) {
            (None, _, _) => true,
            (Some(major), None, _) => version < (major + 1, 0, 0),
            (Some(major), Some(minor), None) => version < (major, minor + 1, 0),
            _ => version <= high.floor(),
        };
        return Some(lower_ok && upper_ok);
    }
    for token in tokens {
        if !comparator_matches(version, token)? {
            return Some(false);
        }
    }
    Some(true)
}

fn comparator_matches(version: (u64, u64, u64), token: &str) -> Option<bool> {
    let (op, rest) = if let Some(rest) = token.strip_prefix(">=") {
        (">=", rest)
    } else if let Some(rest) = token.strip_prefix("<=") {
        ("<=", rest)
    } else if let Some(rest) = token.strip_prefix('>') {
        (">", rest)
    } else if let Some(rest) = token.strip_prefix('<') {
        ("<", rest)
    } else if let Some(rest) = token.strip_prefix("~>") {
        ("~", rest)
    } else if let Some(rest) = token.strip_prefix('~') {
        ("~", rest)
    } else if let Some(rest) = token.strip_prefix('^') {
        ("^", rest)
    } else if let Some(rest) = token.strip_prefix('=') {
        ("=", rest)
    } else {
        ("", token)
    };

    let partial = parse_partial(rest)?;
    let floor = partial.floor();
    Some(match op {
        "" | "=" => match (partial.major, partial.minor, partial.patch) {
            (None, _, _) => true,
            (Some(major), None, _) => version >= floor && version < (major + 1, 0, 0),
            (Some(major), Some(minor), None) => version >= floor && version < (major, minor + 1, 0),
            _ => version == floor,
        },
        "^" => {
            let upper = if partial.major.is_none() {
                None
            } else if floor.0 > 0 || partial.minor.is_none() {
                Some((floor.0 + 1, 0, 0))
            } else if floor.1 > 0 || partial.patch.is_none() {
                Some((0, floor.1 + 1, 0))
            } else {
                Some((0, 0, floor.2 + 1))
            };
            match upper {
                None => true,
                Some(upper) => version >= floor && version < upper,
            }
        }
        "~" => {
            let upper = if partial.major.is_none() {
                None
            } else if partial.minor.is_none() {
                Some((floor.0 + 1, 0, 0))
            } else {
                Some((floor.0, floor.1 + 1, 0))
            };
            match upper {
                None => true,
                Some(upper) => version >= floor && version < upper,
            }
        }
        ">=" => version >= floor,
        ">" => match (partial.major, partial.minor, partial.patch) {
            (None, _, _) => false,
            (Some(major), None, _) => version >= (major + 1, 0, 0),
            (Some(major), Some(minor), None) => version >= (major, minor + 1, 0),
            _ => version > floor,
        },
        "<" => version < floor,
        "<=" => match (partial.major, partial.minor, partial.patch) {
            (None, _, _) => true,
            (Some(major), None, _) => version < (major + 1, 0, 0),
            (Some(major), Some(minor), None) => version < (major, minor + 1, 0),
            _ => version <= floor,
        },
        _ => unreachable!(),
    })
}

// ---------------------------------------------------------------------------
// Build/start command inference

fn get_build_command(
    package_json: Option<&JsonMap>,
    package_manager: PackageManager,
    framework: Option<NodeFramework>,
    explicit_build_command: Option<&str>,
) -> Result<Option<String>> {
    let mut build_command: Option<String> = explicit_build_command.map(str::to_owned);

    let scripts = package_scripts(package_json);
    if build_command.is_none() && scripts.contains_key("build") {
        build_command = Some(package_manager.run_command("build"));
    }
    if build_command.is_none() && framework == Some(NodeFramework::Next) {
        build_command = Some(package_manager.run_execute_command("next build"));
    }

    match (build_command, framework) {
        (Some(command), Some(framework)) => framework
            .bundle_build_command(package_manager, &command)
            .map(Some),
        (command, _) => Ok(command),
    }
}

pub(crate) fn infer_start_command(
    path: &Path,
    package_json: Option<&JsonMap>,
    warn: bool,
    operation: &OperationContext,
) -> Result<Option<String>> {
    let scripts = package_scripts(package_json);
    if let Some(start_script) = scripts.get("start").copied() {
        let trimmed = start_script.trim();
        if !trimmed.is_empty() {
            return Ok(Some(trimmed.to_owned()));
        }
    }

    if let Some(main) = package_json
        .and_then(|pj| pj.get("main"))
        .and_then(Value::as_str)
    {
        let trimmed = main.trim();
        if !trimmed.is_empty() {
            return node_entry_command(trimmed).map(Some);
        }
    }

    if warn && path.join("package.json").is_file() {
        operation.emit(crate::event::Event::Diagnostic {
            level: crate::event::DiagnosticLevel::Warning,
            message: "no start or main script found in package.json".to_owned(),
        });
    }

    let require_node_evidence = !path.join("package.json").is_file();
    let command = common_entry_file(path, require_node_evidence)
        .map(node_entry_command)
        .transpose()?;
    if command.is_none() && warn {
        operation.emit(crate::event::Event::Diagnostic {
            level: crate::event::DiagnosticLevel::Warning,
            message: format!(
                "no entry file found for Node project (tried: {})",
                COMMON_ENTRY_FILES.join(", ")
            ),
        });
    }
    Ok(command)
}

fn node_entry_command(entry_file: &str) -> Result<String> {
    let quoted_entry =
        shlex::try_quote(entry_file).context("Cannot shell-quote Node entry file")?;
    Ok(format!("node {quoted_entry}"))
}

pub(crate) fn split_command(command: &str) -> Vec<String> {
    shlex::split(command).unwrap_or_else(|| command.split_whitespace().map(str::to_owned).collect())
}

// ---------------------------------------------------------------------------
// load_config

pub fn load_config(
    path: &Path,
    base: BaseConfig,
    operation: &OperationContext,
) -> Result<NodeConfig> {
    let mut config = NodeConfig::from_env(base, operation)?;

    let package_manager = config
        .package_manager
        .unwrap_or_else(|| detect_package_manager(path));
    config.package_manager = Some(package_manager);

    let package_json = parse_package_json(path);
    let install_context = discover_js_install_context(path);
    if install_context.requires_all_files {
        config.install_requires_all_files = true;
    }

    let found_deps = check_package_json_deps(package_json.as_ref(), FRAMEWORK_DEPENDENCIES);
    if config.framework.is_none() {
        config.framework = detect_framework(package_json.as_ref(), &found_deps, Some(path));
    }

    if non_empty(&config.build_command).is_none() {
        config.build_command = get_build_command(
            package_json.as_ref(),
            package_manager,
            config.framework,
            non_empty(&config.base.commands.build)
                .map(str::to_owned)
                .as_deref(),
        )?;
    }

    // infer_start=True in the Python default path.
    if non_empty(&config.base.commands.start).is_none() {
        if config.framework == Some(NodeFramework::Nitro) {
            config.base.commands.start =
                infer_start_command(path, package_json.as_ref(), false, operation)?;
        }
        if non_empty(&config.base.commands.start).is_none() {
            if let Some(framework) = config.framework {
                config.base.commands.start = framework.start_command().map(str::to_owned);
            }
        }
        if non_empty(&config.base.commands.start).is_none() {
            config.base.commands.start =
                infer_start_command(path, package_json.as_ref(), true, operation)?;
        }
    }

    if config.framework.is_some()
        && non_empty(&config.base.commands.build).is_some()
        && non_empty(&config.build_command).is_some()
    {
        config.base.commands.build = config.build_command.clone();
    }

    // apply_static_snapshot
    config.install_inputs = Some(install_context.inputs);
    if let Some(name) = package_json_name(package_json.as_ref()) {
        config.package_name = Some(name);
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    //! Port of `tests/test_node_provider.py`.
    //!
    //! Not ported (plan-level / out of scope for this crate):
    //! - `test_node_build_steps_optimize_deps_prunes_then_node_modules`,
    //!   `test_node_provider_uses_build_only_assets_mount`: assert build
    //!   step/mount ordering on evaluated plans — snapshot-gated.
    //! - `test_node_modules_binary_optimizer_removes_executable_binaries`:
    //!   exercises the `optimize-node-modules.sh` bash asset, not provider
    //!   logic ported to Rust.
    //! - `test_laravel_reuses_node_provider_without_static_serving`:
    //!   laravel provider + evaluated plan (laravel.rs is covered
    //!   elsewhere; plan behavior is snapshot-gated).

    use super::*;
    use std::path::PathBuf;

    use crate::providers::base::BaseConfig;

    fn load_config(path: &Path, base: BaseConfig) -> NodeConfig {
        super::load_config(path, base, &OperationContext::for_test()).unwrap()
    }

    fn detect(path: &Path, base: &BaseConfig) -> Option<DetectResult> {
        super::detect(path, base, &OperationContext::for_test())
    }

    fn example(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .join(name)
    }

    fn write(path: &Path, contents: &str) {
        std::fs::write(path, contents).unwrap();
    }

    fn json_map(value: Value) -> JsonMap {
        match value {
            Value::Object(map) => map,
            _ => panic!("expected object"),
        }
    }

    // -----------------------------------------------------------------
    // tests/test_node_provider.py

    #[test]
    fn test_node_package_manager_lockfile_selection() {
        for (lockfile, package_manager) in [
            ("package-lock.json", PackageManager::Npm),
            ("pnpm-lock.yaml", PackageManager::Pnpm),
            ("yarn.lock", PackageManager::Yarn),
            ("bun.lock", PackageManager::Bun),
            ("bun.lockb", PackageManager::Bun),
        ] {
            let tmp = tempfile::tempdir().unwrap();
            write(&tmp.path().join("package.json"), "{}\n");
            write(&tmp.path().join(lockfile), "\n");

            let config = load_config(tmp.path(), BaseConfig::default());

            assert_eq!(config.package_manager, Some(package_manager), "{lockfile}");
        }
    }

    #[test]
    fn test_node_package_manager_defaults_to_npm() {
        let tmp = tempfile::tempdir().unwrap();
        write(&tmp.path().join("package.json"), "{}\n");

        let config = load_config(tmp.path(), BaseConfig::default());

        assert_eq!(config.package_manager, Some(PackageManager::Npm));
    }

    #[test]
    fn test_node_package_manager_uses_package_manager_field() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("package.json"),
            "{\"packageManager\": \"pnpm@10.0.0\"}\n",
        );
        write(&tmp.path().join("package-lock.json"), "{}\n");

        let config = load_config(tmp.path(), BaseConfig::default());

        assert_eq!(config.package_manager, Some(PackageManager::Pnpm));
    }

    #[test]
    fn test_node_package_manager_uses_pnpm_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        write(&tmp.path().join("package.json"), "{}\n");
        write(&tmp.path().join("package-lock.json"), "{}\n");
        write(
            &tmp.path().join("pnpm-workspace.yaml"),
            "packages:\n  - apps/*\n",
        );

        let config = load_config(tmp.path(), BaseConfig::default());

        assert_eq!(config.package_manager, Some(PackageManager::Pnpm));
    }

    #[test]
    fn test_node_check_deps_returns_matching_dependencies() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("package.json"),
            "{\n  \"dependencies\": {\n    \"express\": \"5.1.0\"\n  },\n  \"devDependencies\": {\n    \"hono\": \"^4.12.23\"\n  }\n}\n",
        );

        let package_json = parse_package_json(tmp.path());
        let found_deps =
            check_package_json_deps(package_json.as_ref(), &["express", "elysia", "hono"]);

        assert_eq!(found_deps, BTreeSet::from(["express", "hono"]));
    }

    #[test]
    fn test_node_detection_does_not_beat_node_static() {
        let path = example("nodestatic-vitepress");
        let base = BaseConfig::default();

        let node_static_result =
            crate::providers::node_static::detect(&path, &base, &OperationContext::for_test());
        let node_result = detect(&path, &base);

        let node_static_result = node_static_result.expect("node-static detects");
        let node_result = node_result.expect("node detects");
        assert!(node_static_result.score > node_result.score);
        assert_eq!(
            crate::providers::load_provider_for_test(&path, &base, None).unwrap(),
            "node-static"
        );
    }

    #[test]
    fn test_node_provider_detects_generic_node_example() {
        let path = example("node");

        assert_eq!(
            crate::providers::load_provider_for_test(&path, &BaseConfig::default(), None).unwrap(),
            "node"
        );
    }

    #[test]
    fn test_common_entry_without_package_json_needs_node_evidence() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("app.js"),
            "import http from \"node:http\";\n\nhttp.createServer((_req, res) => {\n  res.end(\"ok\");\n}).listen(process.env.PORT || 8080);\n",
        );

        let config = load_config(tmp.path(), BaseConfig::default());

        assert_eq!(
            crate::providers::load_provider_for_test(tmp.path(), &BaseConfig::default(), None)
                .unwrap(),
            "node"
        );
        assert_eq!(config.base.commands.start.as_deref(), Some("node app.js"));
    }

    #[test]
    fn test_node_provider_detects_node_runtime_examples() {
        for (example_name, framework) in [
            ("node-fastify", NodeFramework::Fastify),
            ("node-hono", NodeFramework::Hono),
            ("node-express", NodeFramework::Express),
            ("node-koa", NodeFramework::Koa),
            ("node-h3", NodeFramework::H3),
            ("node-elysia", NodeFramework::Elysia),
            ("node-nestjs", NodeFramework::Nestjs),
            ("node-nitro", NodeFramework::Nitro),
            ("node-hydrogen", NodeFramework::Hydrogen),
            ("node-react-router", NodeFramework::ReactRouter),
            ("node-remix", NodeFramework::Remix),
            ("node-solidstart", NodeFramework::Solidstart),
            ("node-tanstack-start", NodeFramework::TanstackStart),
            ("node-xmcp", NodeFramework::Xmcp),
            ("node-mastra", NodeFramework::Mastra),
        ] {
            let path = example(example_name);

            assert_eq!(
                crate::providers::load_provider_for_test(&path, &BaseConfig::default(), None)
                    .unwrap(),
                "node",
                "{example_name}"
            );
            let config = load_config(&path, BaseConfig::default());
            assert_eq!(config.framework, Some(framework), "{example_name}");
            assert_eq!(
                config.base.commands.start.as_deref(),
                Some("node server.js"),
                "{example_name}"
            );
        }
    }

    #[test]
    fn test_node_provider_detects_astro_runtime_example() {
        let path = example("node-astro");

        assert_eq!(
            crate::providers::load_provider_for_test(&path, &BaseConfig::default(), None).unwrap(),
            "node"
        );
    }

    #[test]
    fn test_node_provider_detects_hydrogen_config_file() {
        let tmp = tempfile::tempdir().unwrap();
        write(&tmp.path().join("package.json"), "{}\n");
        write(
            &tmp.path().join("hydrogen.config.ts"),
            "export default {}\n",
        );

        let config = load_config(tmp.path(), BaseConfig::default());

        assert_eq!(
            crate::providers::load_provider_for_test(tmp.path(), &BaseConfig::default(), None)
                .unwrap(),
            "node"
        );
        assert_eq!(config.framework, Some(NodeFramework::Hydrogen));
    }

    #[test]
    fn test_node_provider_detects_elysia_runtime_example() {
        let path = example("node-elysia");

        assert_eq!(
            crate::providers::load_provider_for_test(&path, &BaseConfig::default(), None).unwrap(),
            "node"
        );
        let config = load_config(&path, BaseConfig::default());
        assert_eq!(config.framework, Some(NodeFramework::Elysia));
        assert_eq!(
            config.base.commands.start.as_deref(),
            Some("node server.js")
        );
    }

    #[test]
    fn test_node_provider_detects_nextjs_runtime_app() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("package.json"),
            "{\n  \"scripts\": {\n    \"build\": \"next build\",\n    \"start\": \"next start\"\n  },\n  \"dependencies\": {\n    \"next\": \"^14.2.14\",\n    \"react\": \"^18.3.1\",\n    \"react-dom\": \"^18.3.1\"\n  }\n}\n",
        );

        let config = load_config(tmp.path(), BaseConfig::default());

        assert_eq!(
            crate::providers::load_provider_for_test(tmp.path(), &BaseConfig::default(), None)
                .unwrap(),
            "node"
        );
        assert_eq!(config.framework, Some(NodeFramework::Next));
        assert_eq!(
            config.build_command.as_deref(),
            Some("npx -y next-bundle@1.0.0 --build-command 'npm run build'")
        );
        assert_eq!(
            config.base.commands.start.as_deref(),
            Some("node server.mjs")
        );
    }

    #[test]
    fn test_nextjs_build_command_uses_package_manager() {
        for (lockfile, build_command) in [
            (
                "package-lock.json",
                "npx -y next-bundle@1.0.0 --build-command 'npm run build'",
            ),
            (
                "pnpm-lock.yaml",
                "pnpm dlx next-bundle@1.0.0 --build-command 'pnpm run build'",
            ),
            (
                "yarn.lock",
                "yarn dlx next-bundle@1.0.0 --build-command 'yarn run build'",
            ),
            (
                "bun.lock",
                "bunx next-bundle@1.0.0 --build-command 'bun run build'",
            ),
            (
                "bun.lockb",
                "bunx next-bundle@1.0.0 --build-command 'bun run build'",
            ),
        ] {
            let tmp = tempfile::tempdir().unwrap();
            write(
                &tmp.path().join("package.json"),
                "{\n  \"scripts\": {\n    \"build\": \"next build\"\n  },\n  \"dependencies\": {\n    \"next\": \"^14.2.14\"\n  }\n}\n",
            );
            write(&tmp.path().join(lockfile), "\n");

            let config = load_config(tmp.path(), BaseConfig::default());

            assert_eq!(
                config.build_command.as_deref(),
                Some(build_command),
                "{lockfile}"
            );
        }
    }

    #[test]
    fn test_nextjs_build_command_wraps_explicit_build_command() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("package.json"),
            "{\n  \"dependencies\": {\n    \"next\": \"^14.2.14\"\n  }\n}\n",
        );
        let mut base = BaseConfig::default();
        base.commands.build = Some("next build --debug".to_owned());

        let config = load_config(tmp.path(), base);

        assert_eq!(
            config.build_command.as_deref(),
            Some("npx -y next-bundle@1.0.0 --build-command 'next build --debug'")
        );
        assert_eq!(config.base.commands.build, config.build_command);
    }

    #[test]
    fn test_nextjs_build_command_rejects_nul_byte() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("package.json"),
            "{\"dependencies\": {\"next\": \"^14.2.14\"}}\n",
        );
        let mut base = BaseConfig::default();
        base.commands.build = Some("next build\0--debug".to_owned());

        let error =
            super::load_config(tmp.path(), base, &OperationContext::for_test()).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("Cannot shell-quote Node build command"),
            "{error:#}"
        );
    }

    #[test]
    fn test_node_command_splitting_honors_shell_quotes() {
        assert_eq!(
            split_command("vite build --out-dir 'dist site'"),
            ["vite", "build", "--out-dir", "dist site"]
        );
    }

    #[test]
    fn test_nextjs_start_command_prefers_explicit_command() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("package.json"),
            "{\n  \"scripts\": {\n    \"build\": \"next build\",\n    \"start\": \"next start\"\n  },\n  \"dependencies\": {\n    \"next\": \"^14.2.14\"\n  }\n}\n",
        );
        let mut base = BaseConfig::default();
        base.commands.start = Some("node custom-next-server.js".to_owned());

        let config = load_config(tmp.path(), base);

        assert_eq!(
            config.base.commands.start.as_deref(),
            Some("node custom-next-server.js")
        );
    }

    #[test]
    fn test_node_script_commands_uses_build_by_default() {
        let package_json = json_map(serde_json::json!({
            "scripts": {
                "generate": "node generate.js",
                "build": "node build.js",
            },
        }));

        // NodeProvider._script_commands defaults to preferred=("build",).
        assert_eq!(
            script_commands(Some(&package_json), &["build"]),
            vec!["node build.js"]
        );
    }

    #[test]
    fn test_node_start_command_prefers_explicit_command() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("package.json"),
            "{\"scripts\": {\"start\": \"node server.js\"}}\n",
        );
        let mut base = BaseConfig::default();
        base.commands.start = Some("node custom.js".to_owned());

        let config = load_config(tmp.path(), base);

        assert_eq!(
            config.base.commands.start.as_deref(),
            Some("node custom.js")
        );
    }

    #[test]
    fn test_node_start_command_uses_package_script() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("package.json"),
            "{\"scripts\": {\"start\": \"node server.js\"}}\n",
        );

        let config = load_config(tmp.path(), BaseConfig::default());

        assert_eq!(
            config.base.commands.start.as_deref(),
            Some("node server.js")
        );
    }

    #[test]
    fn test_nitro_infers_node_server_start_command() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("package.json"),
            "{\"scripts\":{\"build\":\"vite build\"},\"dependencies\":{\"nitro\":\"3.0.0\"}}\n",
        );

        let config = load_config(tmp.path(), BaseConfig::default());

        assert_eq!(config.framework, Some(NodeFramework::Nitro));
        assert_eq!(
            config.base.commands.start.as_deref(),
            Some("node .output/server/index.mjs")
        );
    }

    #[test]
    fn test_node_start_command_uses_package_main() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("package.json"),
            "{\"main\": \"src/server.js\"}\n",
        );

        let config = load_config(tmp.path(), BaseConfig::default());

        assert_eq!(
            config.base.commands.start.as_deref(),
            Some("node src/server.js")
        );
    }

    #[test]
    fn test_node_start_command_ignores_empty_script_and_uses_package_main() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("package.json"),
            "{\"scripts\": {\"start\": \"  \"}, \"main\": \"src/server.js\"}\n",
        );

        let config = load_config(tmp.path(), BaseConfig::default());

        assert_eq!(
            config.base.commands.start.as_deref(),
            Some("node src/server.js")
        );
    }

    #[test]
    fn test_node_start_command_uses_common_entry_file() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("server.js"),
            "const http = require(\"http\");\n\nhttp.createServer((_req, res) => {\n  res.end(\"ok\");\n}).listen(8080);\n",
        );

        let config = load_config(tmp.path(), BaseConfig::default());

        assert_eq!(
            config.base.commands.start.as_deref(),
            Some("node server.js")
        );
    }

    /// Python's `test_node_provider_skips_native_binary_optimizer_by_default`
    /// asserts on an evaluated plan; the decisive provider-level fact is the
    /// `remove_native_binaries=False` default (the absence of optimizer
    /// steps/mounts is snapshot-gated).
    #[test]
    fn test_node_provider_skips_native_binary_optimizer_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        write(&tmp.path().join("package.json"), "{}\n");

        let config = load_config(tmp.path(), BaseConfig::default());

        assert_eq!(config.remove_native_binaries, Some(false));
    }

    /// Python's `test_node_prepare_steps_use_precompile_edgejs_flag`
    /// evaluates two plans; the provider-level fact is the
    /// `precompile_edgejs=None` default (the prepare-step rendering is
    /// snapshot-gated).
    #[test]
    fn test_node_prepare_steps_use_precompile_edgejs_flag() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("package.json"),
            "{\"scripts\": {\"start\": \"node server.js\"}}",
        );
        write(&tmp.path().join("server.js"), "console.log('ok')\n");

        let config = load_config(tmp.path(), BaseConfig::default());

        assert_eq!(config.precompile_edgejs, None);
    }

    /// Python also asserts the evaluated plan's copy/env/install steps
    /// (`npm install` with `inputs=None`); those are snapshot-gated. The
    /// decisive provider fact is the escape hatch flag.
    #[test]
    fn test_node_provider_uses_escape_hatch_for_external_local_dependency() {
        let tmp = tempfile::tempdir().unwrap();
        let app_dir = tmp.path().join("app");
        let shared_dir = tmp.path().join("shared");
        std::fs::create_dir(&app_dir).unwrap();
        std::fs::create_dir(&shared_dir).unwrap();
        write(
            &app_dir.join("package.json"),
            "{\"dependencies\": {\"shared\": \"file:../shared\"}}",
        );
        write(&shared_dir.join("package.json"), "{\"name\": \"shared\"}");

        let config = load_config(&app_dir, BaseConfig::default());

        assert!(config.install_requires_all_files);
    }

    /// Python also asserts the narrow lockfile copy + `pnpm install` with
    /// `inputs == ["package.json"]` on the plan; snapshot-gated. The
    /// provider-level facts are the package manager and install inputs.
    #[test]
    fn test_node_provider_keeps_narrow_install_without_local_deps() {
        let tmp = tempfile::tempdir().unwrap();
        write(&tmp.path().join("package.json"), "{}\n");
        write(
            &tmp.path().join("pnpm-lock.yaml"),
            "lockfileVersion: '9.0'\n",
        );

        let config = load_config(tmp.path(), BaseConfig::default());

        assert_eq!(config.package_manager, Some(PackageManager::Pnpm));
        assert!(!config.install_requires_all_files);
        assert_eq!(config.install_inputs, Some(vec!["package.json".to_owned()]));
    }
}
