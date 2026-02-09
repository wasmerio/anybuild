//! Context object for Starlark evaluation.
//!
//! The Ctx struct accumulates all declarations and steps during Starlark
//! evaluation and provides a reference system for connecting objects.

use crate::types::{Mount, Package, Service, Step, Volume};
use allocative::Allocative;
use anyhow::{anyhow, Context as _, Result};
use std::collections::HashMap;
use std::fmt;

/// Build configuration accumulated during Starlark evaluation.
#[derive(Debug, Clone, Default, Allocative)]
pub struct Build {
    pub steps: Vec<String>, // Step references
}

/// Serve configuration for running the application.
#[derive(Debug, Clone, Allocative)]
pub struct Serve {
    pub name: String,
    pub provider: String,
    pub build: Vec<String>, // Step references
    pub deps: Vec<String>,  // Package references
    pub commands: HashMap<String, String>,
    pub cwd: Option<String>,
    pub prepare: Vec<String>, // Step references
    pub workers: Vec<String>, // Command names
    pub mounts: Vec<String>,  // Mount references
    pub volumes: Vec<String>, // Volume references
    pub env: HashMap<String, String>,
    pub services: Vec<String>, // Service references
}

/// Reference type for resolving Ctx objects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CtxRef {
    Package(String), // Key: "name@version"
    Build(usize),    // Index
    Step(usize),     // Index
    Serve(String),   // Name
    Mount(usize),    // Index
    Volume(usize),   // Index
    Service(String), // Name
}

/// Context object that accumulates state during Starlark evaluation.
///
/// All Starlark functions mutate this context to add packages, steps, builds,
/// serves, etc. Objects are stored in the context and references are returned
/// as strings that can be resolved later.
#[derive(Debug, Clone, Allocative)]
pub struct Ctx {
    // Collections indexed by reference strings
    pub packages: HashMap<String, Package>,
    pub builds: Vec<Build>,
    pub steps: Vec<Step>,
    pub serves: HashMap<String, Serve>,
    pub mounts: Vec<Mount>,
    pub volumes: Vec<Volume>,
    pub services: HashMap<String, Service>,
}

impl Default for Ctx {
    fn default() -> Self {
        Self::new()
    }
}

impl Ctx {
    /// Create a new empty context.
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
            builds: Vec::new(),
            steps: Vec::new(),
            serves: HashMap::new(),
            mounts: Vec::new(),
            volumes: Vec::new(),
            services: HashMap::new(),
        }
    }

    /// Add a package and return its reference string.
    ///
    /// Reference format: `ref:package:name@version` or `ref:package:name` if no version.
    pub fn add_package(&mut self, package: Package) -> String {
        let key = if let Some(version) = &package.version {
            format!("{}@{}", package.name, version)
        } else {
            package.name.clone()
        };

        let ref_str = format!("ref:package:{}", key);
        self.packages.insert(key, package);
        ref_str
    }

    /// Add a build and return its reference string.
    ///
    /// Reference format: `ref:build:index`
    pub fn add_build(&mut self, build: Build) -> String {
        let index = self.builds.len();
        self.builds.push(build);
        format!("ref:build:{}", index)
    }

    /// Add a step and return its reference string.
    ///
    /// Reference format: `ref:step:index`
    pub fn add_step(&mut self, step: Step) -> String {
        let index = self.steps.len();
        self.steps.push(step);
        format!("ref:step:{}", index)
    }

    /// Add a serve configuration and return its reference string.
    ///
    /// Reference format: `ref:serve:name`
    pub fn add_serve(&mut self, serve: Serve) -> String {
        let name = serve.name.clone();
        let ref_str = format!("ref:serve:{}", name);
        self.serves.insert(name, serve);
        ref_str
    }

    /// Add a mount and return its reference string.
    ///
    /// Reference format: `ref:mount:index`
    pub fn add_mount(&mut self, mount: Mount) -> String {
        let index = self.mounts.len();
        self.mounts.push(mount);
        format!("ref:mount:{}", index)
    }

    /// Add a volume and return its reference string.
    ///
    /// Reference format: `ref:volume:index`
    pub fn add_volume(&mut self, volume: Volume) -> String {
        let index = self.volumes.len();
        self.volumes.push(volume);
        format!("ref:volume:{}", index)
    }

    /// Add a service and return its reference string.
    ///
    /// Reference format: `ref:service:name`
    pub fn add_service(&mut self, service: Service) -> String {
        let name = service.name.clone();
        let ref_str = format!("ref:service:{}", name);
        self.services.insert(name, service);
        ref_str
    }

    /// Parse a reference string and return the corresponding CtxRef.
    pub fn parse_ref(&self, ref_str: &str) -> Result<CtxRef> {
        let parts: Vec<&str> = ref_str.split(':').collect();

        if parts.len() < 3 || parts[0] != "ref" {
            return Err(anyhow!("Invalid reference format: {}", ref_str));
        }

        match parts[1] {
            "package" => {
                let key = parts[2..].join(":");
                if self.packages.contains_key(&key) {
                    Ok(CtxRef::Package(key))
                } else {
                    Err(anyhow!("Package not found: {}", key))
                }
            }
            "build" => {
                let index = parts[2].parse::<usize>().context("Invalid build index")?;
                if index < self.builds.len() {
                    Ok(CtxRef::Build(index))
                } else {
                    Err(anyhow!("Build index out of range: {}", index))
                }
            }
            "step" => {
                let index = parts[2].parse::<usize>().context("Invalid step index")?;
                if index < self.steps.len() {
                    Ok(CtxRef::Step(index))
                } else {
                    Err(anyhow!("Step index out of range: {}", index))
                }
            }
            "serve" => {
                let name = parts[2..].join(":");
                if self.serves.contains_key(&name) {
                    Ok(CtxRef::Serve(name))
                } else {
                    Err(anyhow!("Serve not found: {}", name))
                }
            }
            "mount" => {
                let index = parts[2].parse::<usize>().context("Invalid mount index")?;
                if index < self.mounts.len() {
                    Ok(CtxRef::Mount(index))
                } else {
                    Err(anyhow!("Mount index out of range: {}", index))
                }
            }
            "volume" => {
                let index = parts[2].parse::<usize>().context("Invalid volume index")?;
                if index < self.volumes.len() {
                    Ok(CtxRef::Volume(index))
                } else {
                    Err(anyhow!("Volume index out of range: {}", index))
                }
            }
            "service" => {
                let name = parts[2..].join(":");
                if self.services.contains_key(&name) {
                    Ok(CtxRef::Service(name))
                } else {
                    Err(anyhow!("Service not found: {}", name))
                }
            }
            _ => Err(anyhow!("Unknown reference type: {}", parts[1])),
        }
    }

    /// Get a package by reference.
    pub fn get_package(&self, ref_str: &str) -> Result<&Package> {
        let ctx_ref = self.parse_ref(ref_str)?;
        match ctx_ref {
            CtxRef::Package(key) => self
                .packages
                .get(&key)
                .ok_or_else(|| anyhow!("Package not found: {}", key)),
            _ => Err(anyhow!("Not a package reference: {}", ref_str)),
        }
    }

    /// Get a step by reference.
    pub fn get_step(&self, ref_str: &str) -> Result<&Step> {
        let ctx_ref = self.parse_ref(ref_str)?;
        match ctx_ref {
            CtxRef::Step(index) => self
                .steps
                .get(index)
                .ok_or_else(|| anyhow!("Step not found at index: {}", index)),
            _ => Err(anyhow!("Not a step reference: {}", ref_str)),
        }
    }

    /// Get a serve by reference.
    pub fn get_serve(&self, ref_str: &str) -> Result<&Serve> {
        let ctx_ref = self.parse_ref(ref_str)?;
        match ctx_ref {
            CtxRef::Serve(name) => self
                .serves
                .get(&name)
                .ok_or_else(|| anyhow!("Serve not found: {}", name)),
            _ => Err(anyhow!("Not a serve reference: {}", ref_str)),
        }
    }

    /// Get a mount by reference.
    pub fn get_mount(&self, ref_str: &str) -> Result<&Mount> {
        let ctx_ref = self.parse_ref(ref_str)?;
        match ctx_ref {
            CtxRef::Mount(index) => self
                .mounts
                .get(index)
                .ok_or_else(|| anyhow!("Mount not found at index: {}", index)),
            _ => Err(anyhow!("Not a mount reference: {}", ref_str)),
        }
    }

    /// Get a volume by reference.
    pub fn get_volume(&self, ref_str: &str) -> Result<&Volume> {
        let ctx_ref = self.parse_ref(ref_str)?;
        match ctx_ref {
            CtxRef::Volume(index) => self
                .volumes
                .get(index)
                .ok_or_else(|| anyhow!("Volume not found at index: {}", index)),
            _ => Err(anyhow!("Not a volume reference: {}", ref_str)),
        }
    }

    /// Get a service by reference.
    pub fn get_service(&self, ref_str: &str) -> Result<&Service> {
        let ctx_ref = self.parse_ref(ref_str)?;
        match ctx_ref {
            CtxRef::Service(name) => self
                .services
                .get(&name)
                .ok_or_else(|| anyhow!("Service not found: {}", name)),
            _ => Err(anyhow!("Not a service reference: {}", ref_str)),
        }
    }
}

impl fmt::Display for Ctx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Ctx(packages={}, steps={}, serves={}, mounts={}, volumes={}, services={})",
            self.packages.len(),
            self.steps.len(),
            self.serves.len(),
            self.mounts.len(),
            self.volumes.len(),
            self.services.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_package() {
        let mut ctx = Ctx::new();
        let package = Package {
            name: "python".to_string(),
            version: Some("3.11".to_string()),
            architecture: None,
        };

        let ref_str = ctx.add_package(package.clone());
        assert_eq!(ref_str, "ref:package:python@3.11");
        assert!(ctx.packages.contains_key("python@3.11"));
    }

    #[test]
    fn test_add_step() {
        let mut ctx = Ctx::new();
        let step = Step::Run(crate::types::RunStep {
            command: "echo hello".to_string(),
            inputs: None,
            outputs: None,
            group: None,
        });

        let ref_str = ctx.add_step(step);
        assert_eq!(ref_str, "ref:step:0");
        assert_eq!(ctx.steps.len(), 1);
    }

    #[test]
    fn test_parse_ref() {
        let mut ctx = Ctx::new();
        let package = Package {
            name: "node".to_string(),
            version: Some("20".to_string()),
            architecture: None,
        };
        ctx.add_package(package);

        let ctx_ref = ctx.parse_ref("ref:package:node@20").unwrap();
        assert_eq!(ctx_ref, CtxRef::Package("node@20".to_string()));
    }

    #[test]
    fn test_get_package() {
        let mut ctx = Ctx::new();
        let package = Package {
            name: "go".to_string(),
            version: Some("1.21".to_string()),
            architecture: None,
        };
        let ref_str = ctx.add_package(package.clone());

        let retrieved = ctx.get_package(&ref_str).unwrap();
        assert_eq!(retrieved.name, "go");
        assert_eq!(retrieved.version, Some("1.21".to_string()));
    }

    #[test]
    fn test_invalid_reference() {
        let ctx = Ctx::new();
        let result = ctx.parse_ref("invalid:ref");
        assert!(result.is_err());
    }
}
