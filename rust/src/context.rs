//! Context objects used during Starlark evaluation and build execution.
//!
//! The Ctx mirrors the Python implementation, tracking references to steps,
//! mounts, volumes, and services while providing helper methods bound into the
//! Starlark runtime.

use std::collections::{BTreeMap, BTreeSet};

use camino::Utf8PathBuf;

use crate::builder::Builder;
use crate::model::{
    Build, Env, Mount, Package, PrepareStep, Serve, Service, ServiceProvider, Step, Volume,
};
/// Stringly-typed reference identifiers to mirror the Python runtime.
pub type RefId = String;

/// Execution/evaluation context for a Shipit file.
pub struct Ctx<'a> {
    pub builder: &'a mut dyn Builder,
    pub packages: BTreeMap<String, Package>,
    pub builds: Vec<Build>,
    pub steps: Vec<Step>,
    pub serves: BTreeMap<String, Serve>,
    pub mounts: Vec<Mount>,
    pub volumes: Vec<Volume>,
    pub services: BTreeMap<String, Service>,
    pub getenv_variables: BTreeSet<String>,
}

impl<'a> Ctx<'a> {
    pub fn new(builder: &'a mut dyn Builder) -> Self {
        Self {
            builder,
            packages: BTreeMap::new(),
            builds: Vec::new(),
            steps: Vec::new(),
            serves: BTreeMap::new(),
            mounts: Vec::new(),
            volumes: Vec::new(),
            services: BTreeMap::new(),
            getenv_variables: BTreeSet::new(),
        }
    }

    fn ref_package_key(pkg: &Package) -> String {
        match (&pkg.version, &pkg.architecture) {
            (Some(version), Some(arch)) => format!("{}({})@{}", pkg.name, arch, version),
            (Some(version), None) => format!("{}@{}", pkg.name, version),
            (None, Some(arch)) => format!("{}({})", pkg.name, arch),
            (None, None) => pkg.name.clone(),
        }
    }

    pub fn add_package(&mut self, package: Package) -> RefId {
        let key = Self::ref_package_key(&package);
        self.packages.insert(key.clone(), package);
        format!("ref:package:{key}")
    }

    pub fn add_service(&mut self, service: Service) -> RefId {
        let name = service.name.clone();
        self.services.insert(name.clone(), service);
        format!("ref:service:{name}")
    }

    pub fn add_build(&mut self, build: Build) -> RefId {
        self.builds.push(build);
        format!("ref:build:{}", self.builds.len() - 1)
    }

    pub fn add_serve(&mut self, serve: Serve) -> RefId {
        let name = serve.name.clone();
        self.serves.insert(name.clone(), serve);
        format!("ref:serve:{name}")
    }

    pub fn add_step(&mut self, step: Step) -> RefId {
        self.steps.push(step);
        format!("ref:step:{}", self.steps.len() - 1)
    }

    pub fn add_mount(&mut self, mount: Mount) -> RefId {
        self.mounts.push(mount);
        format!("ref:mount:{}", self.mounts.len() - 1)
    }

    pub fn add_volume(&mut self, volume: Volume) -> RefId {
        self.volumes.push(volume);
        format!("ref:volume:{}", self.volumes.len() - 1)
    }

    pub fn getenv(&mut self, name: &str) -> Option<String> {
        self.getenv_variables.insert(name.to_string());
        self.builder.getenv(name)
    }

    // Convenience constructors below map to Starlark builtins eventually.

    pub fn dep(
        &mut self,
        name: String,
        version: Option<String>,
        architecture: Option<String>,
    ) -> RefId {
        let package = Package {
            name,
            version,
            architecture,
        };
        self.add_package(package)
    }

    pub fn service(&mut self, name: String, provider: ServiceProvider) -> RefId {
        let service = Service { name, provider };
        self.add_service(service)
    }

    pub fn serve(
        &mut self,
        name: String,
        provider: String,
        build: Vec<RefId>,
        deps: Vec<RefId>,
        commands: BTreeMap<String, String>,
        cwd: Option<String>,
        prepare: Option<Vec<RefId>>,
        workers: Option<Vec<String>>,
        mount_refs: Option<Vec<String>>,
        volume_refs: Option<Vec<String>>,
        env: Option<Env>,
        service_refs: Option<Vec<RefId>>,
    ) -> RefId {
        let build_refs = self.resolve_steps(&build);
        let prepare_steps = prepare
            .as_ref()
            .map(|refs| self.resolve_prepare_steps(refs));
        let dep_refs = deps
            .iter()
            .filter_map(|r| {
                self.packages
                    .get(r.trim_start_matches("ref:package:"))
                    .cloned()
            })
            .collect();
        let mount_objs = mount_refs
            .as_ref()
            .map(|refs| self.resolve_mount_refs(refs))
            .filter(|v| !v.is_empty());
        let volume_objs = volume_refs
            .as_ref()
            .map(|refs| self.resolve_volume_refs(refs))
            .filter(|v| !v.is_empty());
        let serve = Serve {
            name: name.clone(),
            provider,
            build: build_refs,
            deps: dep_refs,
            commands,
            cwd,
            prepare: prepare_steps,
            workers,
            mounts: mount_objs,
            volumes: volume_objs,
            env,
            services: service_refs.map(|refs| self.resolve_services(&refs)),
        };
        self.add_serve(serve)
    }

    pub fn path(&mut self, path: String) -> RefId {
        let step = Step::Path(crate::model::PathStep { path });
        self.add_step(step)
    }

    pub fn use_deps(&mut self, dependencies: Vec<RefId>) -> RefId {
        let deps = dependencies
            .iter()
            .filter_map(|r| {
                self.packages
                    .get(r.trim_start_matches("ref:package:"))
                    .cloned()
            })
            .collect();
        let step = Step::Use(crate::model::UseStep { dependencies: deps });
        self.add_step(step)
    }

    pub fn run(&mut self, step: crate::model::RunStep) -> RefId {
        self.add_step(Step::Run(step))
    }

    pub fn workdir(&mut self, path: Utf8PathBuf) -> RefId {
        let step = Step::Workdir(crate::model::WorkdirStep { path });
        self.add_step(step)
    }

    pub fn copy(&mut self, step: crate::model::CopyStep) -> RefId {
        self.add_step(Step::Copy(step))
    }

    pub fn env_step(&mut self, variables: Env) -> RefId {
        let step = Step::Env(crate::model::EnvStep { variables });
        self.add_step(step)
    }

    pub fn mount(&mut self, name: String) -> (RefId, Utf8PathBuf, Utf8PathBuf) {
        let build_path = self.builder.get_build_mount_path(&name);
        let serve_path = self.builder.get_serve_mount_path(&name);
        let mount = Mount {
            name,
            build_path: build_path.clone(),
            serve_path: serve_path.clone(),
        };
        let ref_id = self.add_mount(mount);
        (ref_id, build_path, serve_path)
    }

    pub fn volume(&mut self, name: String, serve: Utf8PathBuf) -> (RefId, String) {
        let volume = Volume {
            name: name.clone(),
            serve_path: serve,
        };
        let ref_id = self.add_volume(volume);
        (ref_id, name)
    }

    fn resolve_steps(&self, refs: &[RefId]) -> Vec<Step> {
        refs.iter()
            .filter_map(|r| {
                self.parse_index(r, "ref:step:")
                    .and_then(|idx| self.steps.get(idx).cloned())
            })
            .collect()
    }

    fn resolve_prepare_steps(&self, refs: &[RefId]) -> Vec<PrepareStep> {
        self.resolve_steps(refs)
            .into_iter()
            .filter_map(|s| match s {
                Step::Run(run) => Some(run),
                _ => None,
            })
            .collect()
    }

    fn resolve_services(&self, refs: &[RefId]) -> Vec<Service> {
        refs.iter()
            .filter_map(|r| {
                self.parse_name(r, "ref:service:")
                    .and_then(|name| self.services.get(&name).cloned())
            })
            .collect()
    }

    fn resolve_mount_refs(&self, refs: &[String]) -> Vec<Mount> {
        refs.iter()
            .filter_map(|r| {
                self.parse_index(r, "ref:mount:")
                    .and_then(|idx| self.mounts.get(idx).cloned())
            })
            .collect()
    }

    fn resolve_volume_refs(&self, refs: &[String]) -> Vec<Volume> {
        refs.iter()
            .filter_map(|r| {
                self.parse_index(r, "ref:volume:")
                    .and_then(|idx| self.volumes.get(idx).cloned())
            })
            .collect()
    }

    fn parse_index(&self, value: &str, prefix: &str) -> Option<usize> {
        value
            .strip_prefix(prefix)
            .and_then(|rest| rest.parse().ok())
    }

    fn parse_name(&self, value: &str, prefix: &str) -> Option<String> {
        value.strip_prefix(prefix).map(|s| s.to_string())
    }
}
