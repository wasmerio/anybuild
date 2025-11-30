use std::fs;
use std::path::Path;

use camino::Utf8PathBuf;
use serde_json::Value;

use crate::Result;
use crate::model::{CustomCommands, DependencySpec, DetectResult, MountSpec, ProviderPlan};
use crate::provider::{Provider, ProviderDescriptor, apply_custom_commands};

#[derive(Debug, Clone, Copy)]
enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl PackageManager {
    fn lockfile(&self) -> &'static str {
        match self {
            PackageManager::Npm => "package-lock.json",
            PackageManager::Pnpm => "pnpm-lock.yaml",
            PackageManager::Yarn => "yarn.lock",
            PackageManager::Bun => "bun.lockb",
        }
    }

    fn install_command(&self, has_lock: bool) -> String {
        match self {
            PackageManager::Npm => {
                if has_lock {
                    "npm ci".to_string()
                } else {
                    "npm install".to_string()
                }
            }
            PackageManager::Pnpm => "pnpm install".to_string(),
            PackageManager::Yarn => "yarn install".to_string(),
            PackageManager::Bun => {
                if has_lock {
                    "bun install --no-save".to_string()
                } else {
                    "bun install".to_string()
                }
            }
        }
    }

    fn run_cmd(&self, script: &str) -> String {
        match self {
            PackageManager::Npm => format!("npm run {script}"),
            PackageManager::Pnpm => format!("pnpm run {script}"),
            PackageManager::Yarn => format!("yarn run {script}"),
            PackageManager::Bun => format!("bun run {script}"),
        }
    }

    fn run_exec(&self, bin: &str) -> String {
        match self {
            PackageManager::Npm => format!("npx {bin}"),
            PackageManager::Pnpm => format!("pnpx {bin}"),
            PackageManager::Yarn => format!("ypx {bin}"),
            PackageManager::Bun => format!("bunx {bin}"),
        }
    }

    fn dependency(&self, path: &Path) -> DependencySpec {
        let name = match self {
            PackageManager::Npm => "npm",
            PackageManager::Pnpm => "pnpm",
            PackageManager::Yarn => "yarn",
            PackageManager::Bun => "bun",
        }
        .to_string();
        let mut default_version = None;
        if matches!(self, PackageManager::Pnpm) {
            if let Some(lock_ver) = Self::pnpm_lock_version(&path.join(self.lockfile())) {
                if lock_ver.starts_with("5.") {
                    default_version = Some("7".to_string());
                } else if lock_ver.starts_with("6.") {
                    default_version = Some("8".to_string());
                }
            }
        }
        DependencySpec {
            name,
            env_var: Some("SHIPIT_NPM_VERSION".to_string())
                .filter(|_| matches!(self, PackageManager::Npm))
                .or_else(|| {
                    matches!(self, PackageManager::Pnpm).then(|| "SHIPIT_PNPM_VERSION".to_string())
                })
                .or_else(|| {
                    matches!(self, PackageManager::Yarn).then(|| "SHIPIT_YARN_VERSION".to_string())
                })
                .or_else(|| {
                    matches!(self, PackageManager::Bun).then(|| "SHIPIT_BUN_VERSION".to_string())
                }),
            default_version,
            architecture_var: None,
            alias: None,
            use_in_build: true,
            use_in_serve: false,
        }
    }

    fn pnpm_lock_version(lockfile: &Path) -> Option<String> {
        if !lockfile.exists() {
            return None;
        }
        let data = fs::read_to_string(lockfile).ok()?;
        for line in data.lines() {
            if line.contains("lockfileVersion") {
                if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(line) {
                    if let Some(ver) = val.get("lockfileVersion").and_then(|v| v.as_str()) {
                        return Some(ver.to_string());
                    }
                }
            }
        }
        None
    }
}

#[derive(Debug, Clone, Copy)]
enum StaticGenerator {
    Astro,
    Vite,
    Next,
    Gatsby,
    DocusaurusOld,
    Docusaurus,
    Svelte,
    RemixOld,
    RemixV2,
    NuxtOld,
    NuxtV3,
}

pub struct NodeStaticProvider {
    path: Utf8PathBuf,
    package_manager: PackageManager,
    _package_json: Option<Value>,
    static_generator: Option<StaticGenerator>,
    build_command: Option<String>,
    custom_commands: CustomCommands,
}

impl NodeStaticProvider {
    pub fn new(path: &Path, custom: &CustomCommands) -> Result<Self> {
        let path_buf = Utf8PathBuf::from_path_buf(path.to_path_buf())
            .map_err(|_| anyhow::anyhow!("Invalid UTF-8 path"))?;
        let package_manager = if path_buf.join("package-lock.json").exists() {
            PackageManager::Npm
        } else if path_buf.join("pnpm-lock.yaml").exists() {
            PackageManager::Pnpm
        } else if path_buf.join("yarn.lock").exists() {
            PackageManager::Yarn
        } else if path_buf.join("bun.lockb").exists() {
            PackageManager::Bun
        } else {
            PackageManager::Npm
        };
        let package_json = read_package_json(&path_buf);
        let static_generator = detect_static_generator(&package_json);
        let build_command = compute_build_command(&package_json, package_manager, static_generator);
        Ok(Self {
            path: path_buf,
            package_manager,
            _package_json: package_json,
            static_generator,
            build_command,
            custom_commands: custom.clone(),
        })
    }
}

impl Provider for NodeStaticProvider {
    fn name(&self) -> &'static str {
        "node-static"
    }

    fn platform(&self) -> Option<&str> {
        self.static_generator.as_ref().map(|g| match g {
            StaticGenerator::Astro => "astro",
            StaticGenerator::Vite => "vite",
            StaticGenerator::Next => "next",
            StaticGenerator::Gatsby => "gatsby",
            StaticGenerator::DocusaurusOld => "docusaurus-old",
            StaticGenerator::Docusaurus => "docusaurus",
            StaticGenerator::Svelte => "svelte",
            StaticGenerator::RemixOld => "remix-old",
            StaticGenerator::RemixV2 => "remix-v2",
            StaticGenerator::NuxtOld => "nuxt",
            StaticGenerator::NuxtV3 => "nuxt3",
        })
    }

    fn plan(&self) -> Result<ProviderPlan> {
        let lockfile = self.package_manager.lockfile().to_string();
        let has_lock = self.path.join(&lockfile).exists();
        let install_cmd = self.package_manager.install_command(has_lock);
        let build_cmd = self
            .build_command
            .clone()
            .unwrap_or_else(|| self.package_manager.run_cmd("build"));
        let output_dir = output_dir(self.static_generator);

        let deps = vec![
            DependencySpec {
                name: "node".to_string(),
                env_var: Some("SHIPIT_NODE_VERSION".to_string()),
                default_version: Some("22".to_string()),
                architecture_var: None,
                alias: None,
                use_in_build: true,
                use_in_serve: false,
            },
            self.package_manager.dependency(self.path.as_std_path()),
            DependencySpec {
                name: "static-web-server".to_string(),
                env_var: Some("SHIPIT_SWS_VERSION".to_string()),
                default_version: Some("2.38.0".to_string()),
                architecture_var: None,
                alias: None,
                use_in_build: false,
                use_in_serve: true,
            },
        ];
        // ensure package manager dep is marked serve false already.
        let mut build_steps: Vec<String> = Vec::new();
        build_steps.push("workdir(temp[\"build\"])".to_string());
        if has_lock {
            build_steps.push(format!("copy({})", serde_json::to_string(&lockfile)?));
        }
        build_steps.push(format!(
            "run({}, inputs=[\"package.json\"], group=\"install\")",
            serde_json::to_string(&install_cmd)?
        ));
        let mut ignore = vec!["node_modules".to_string(), ".git".to_string()];
        if has_lock {
            ignore.push(lockfile.clone());
        }
        build_steps.push(format!(
            "copy(\".\", ignore=[{}])",
            ignore
                .iter()
                .map(|s| serde_json::to_string(s).unwrap())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        build_steps.push(format!(
            "run({}, outputs=[shipit_static_dir], group=\"build\")",
            serde_json::to_string(&build_cmd)?
        ));
        build_steps
            .push("run(\"cp -R {}/* {}/\".format(shipit_static_dir, app[\"build\"]))".to_string());

        let mut mounts = vec![MountSpec {
            name: "temp".to_string(),
            attach_to_build: true,
            attach_to_serve: false,
        }];
        mounts.push(MountSpec {
            name: "app".to_string(),
            attach_to_build: true,
            attach_to_serve: true,
        });

        let mut commands = std::collections::BTreeMap::new();
        commands.insert(
            "start".to_string(),
            "\"static-web-server --root={} --log-level=info --port={}\".format(app[\"serve\"], PORT)".to_string(),
        );
        let commands = apply_custom_commands(&self.custom_commands, commands)?;

        Ok(ProviderPlan {
            serve_name: self
                .path
                .file_name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "app".to_string()),
            cwd: None,
            provider: self.name().to_string(),
            mounts,
            platform: self.platform().map(|s| s.to_string()),
            volumes: Vec::new(),
            declarations: Some(format!(
                "shipit_static_dir = getenv(\"SHIPIT_STATIC_DIR\") or \"{}\"",
                output_dir
            )),
            dependencies: deps,
            build_steps,
            prepare: None,
            services: Vec::new(),
            commands,
            env: None,
        })
    }
}

pub struct NodeStaticDescriptor;

impl ProviderDescriptor for NodeStaticDescriptor {
    fn detect(&self, path: &Path, _custom: &CustomCommands) -> Option<DetectResult> {
        let package_json = read_package_json_path(path);
        if package_json.is_none() {
            return None;
        }
        let generators = [
            "astro",
            "vite",
            "next",
            "nuxt",
            "gatsby",
            "svelte",
            "docusaurus",
            "@docusaurus/core",
            "@remix-run/dev",
        ];
        if generators
            .iter()
            .any(|dep| has_dependency(&package_json, dep, None))
        {
            return Some(DetectResult {
                name: "node-static".to_string(),
                score: 40,
            });
        }
        None
    }

    fn create(&self, path: &Path, custom: &CustomCommands) -> Result<Box<dyn Provider>> {
        Ok(Box::new(NodeStaticProvider::new(path, custom)?))
    }

    fn name(&self) -> &'static str {
        "node-static"
    }
}

fn read_package_json_path(path: &Path) -> Option<Value> {
    let path_buf = Utf8PathBuf::from_path_buf(path.to_path_buf()).ok()?;
    read_package_json(&path_buf)
}

fn read_package_json(path: &Utf8PathBuf) -> Option<Value> {
    let file = path.join("package.json");
    if !file.exists() {
        return None;
    }
    let text = fs::read_to_string(file).ok()?;
    serde_json::from_str(&text).ok()
}

fn has_dependency(pkg: &Option<Value>, name: &str, version: Option<&str>) -> bool {
    let obj = match pkg {
        Some(Value::Object(map)) => map,
        _ => return false,
    };
    let sections = ["dependencies", "devDependencies", "peerDependencies"];
    for section in &sections {
        if let Some(Value::Object(deps)) = obj.get(*section) {
            if let Some(Value::String(val)) = deps.get(name) {
                if let Some(ver) = version {
                    if val.contains(ver) {
                        return true;
                    }
                } else {
                    return true;
                }
            }
        }
    }
    false
}

fn detect_static_generator(pkg: &Option<Value>) -> Option<StaticGenerator> {
    let order = [
        (StaticGenerator::Gatsby, "gatsby", None),
        (StaticGenerator::Astro, "astro", None),
        (StaticGenerator::DocusaurusOld, "docusaurus", None),
        (StaticGenerator::Docusaurus, "@docusaurus/core", None),
        (StaticGenerator::Svelte, "svelte", None),
        (StaticGenerator::RemixOld, "@remix-run/dev", Some("0")),
        (StaticGenerator::RemixOld, "@remix-run/dev", Some("1")),
        (StaticGenerator::RemixV2, "@remix-run/dev", None),
        (StaticGenerator::Vite, "vite", None),
        (StaticGenerator::Next, "next", None),
        (StaticGenerator::NuxtOld, "nuxt", Some("1")),
        (StaticGenerator::NuxtOld, "nuxt", Some("2")),
        (StaticGenerator::NuxtV3, "nuxt", None),
    ];
    for (generator, dep, ver) in &order {
        if has_dependency(pkg, dep, ver.as_deref()) {
            return Some(*generator);
        }
    }
    None
}

fn compute_build_command(
    pkg: &Option<Value>,
    manager: PackageManager,
    generator: Option<StaticGenerator>,
) -> Option<String> {
    if let Some(Value::Object(obj)) = pkg {
        if let Some(Value::Object(scripts)) = obj.get("scripts") {
            if scripts.get("build").is_some() {
                return Some(manager.run_cmd("build"));
            }
        }
    }
    match generator {
        Some(StaticGenerator::Gatsby) => Some(manager.run_exec("gatsby build")),
        Some(StaticGenerator::Astro) => Some(manager.run_exec("astro build")),
        Some(StaticGenerator::RemixOld) => Some(manager.run_exec("remix-ssg build")),
        Some(StaticGenerator::RemixV2) => Some(manager.run_exec("vite build")),
        Some(StaticGenerator::Docusaurus) | Some(StaticGenerator::DocusaurusOld) => {
            Some(manager.run_exec("docusaurus build"))
        }
        Some(StaticGenerator::Svelte) => Some(manager.run_exec("svelte-kit build")),
        Some(StaticGenerator::Vite) => Some(manager.run_exec("vite build")),
        Some(StaticGenerator::Next) => Some(manager.run_exec("next export")),
        Some(StaticGenerator::NuxtV3) => Some(manager.run_exec("nuxi generate")),
        Some(StaticGenerator::NuxtOld) => Some(manager.run_exec("nuxt generate")),
        None => None,
    }
}

fn output_dir(generator: Option<StaticGenerator>) -> &'static str {
    match generator {
        Some(StaticGenerator::Next) => "out",
        Some(StaticGenerator::Astro)
        | Some(StaticGenerator::Vite)
        | Some(StaticGenerator::NuxtOld)
        | Some(StaticGenerator::NuxtV3)
        | Some(StaticGenerator::RemixV2) => "dist",
        Some(StaticGenerator::Gatsby) => "public",
        Some(StaticGenerator::RemixOld) => "build/client",
        Some(StaticGenerator::Docusaurus)
        | Some(StaticGenerator::DocusaurusOld)
        | Some(StaticGenerator::Svelte) => "build",
        None => "dist",
    }
}
