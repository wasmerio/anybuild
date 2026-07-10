//! Shipit plan types — the Rust port of `shipit_types.py`.
//!
//! Plan-visible maps (env vars, commands) use `IndexMap`: Python dicts
//! preserve insertion order and plan JSON is the compatibility contract.

use std::fmt;
use std::path::PathBuf;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

pub mod layout;
pub mod snapshot;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mount {
    pub name: String,
    pub build_path: PathBuf,
    pub serve_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Volume {
    pub name: String,
    pub path: PathBuf,
    pub serve_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Service {
    pub name: String,
    /// "postgres" | "mysql" | "redis"
    pub provider: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: Option<String>,
    /// "64-bit" | "32-bit"
    pub architecture: Option<String>,
}

impl fmt::Display for Package {
    /// Matches Python's `Package.__str__`: `name(arch)@version`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.architecture {
            Some(arch) => write!(f, "{}({})", self.name, arch)?,
            None => write!(f, "{}", self.name)?,
        }
        if let Some(version) = &self.version {
            write!(f, "@{}", version)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunStep {
    pub command: String,
    pub inputs: Option<Vec<String>>,
    pub outputs: Option<Vec<String>>,
    pub group: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkdirStep {
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CopyStep {
    pub source: String,
    pub target: String,
    pub ignore: Option<Vec<String>>,
    /// "source" | "assets"
    pub base: String,
}

impl CopyStep {
    pub fn is_download(&self) -> bool {
        self.source.starts_with("http://") || self.source.starts_with("https://")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvStep {
    pub variables: IndexMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UseStep {
    pub dependencies: Vec<Package>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathStep {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WriteFileStep {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Step {
    Run(RunStep),
    Copy(CopyStep),
    Env(EnvStep),
    Path(PathStep),
    Use(UseStep),
    Workdir(WorkdirStep),
    WriteFile(WriteFileStep),
}

impl Step {
    /// The Python dataclass name, used as `__type__` in normalized plans.
    pub fn type_name(&self) -> &'static str {
        match self {
            Step::Run(_) => "RunStep",
            Step::Copy(_) => "CopyStep",
            Step::Env(_) => "EnvStep",
            Step::Path(_) => "PathStep",
            Step::Use(_) => "UseStep",
            Step::Workdir(_) => "WorkdirStep",
            Step::WriteFile(_) => "WriteFileStep",
        }
    }
}

/// A prepared serve: the evaluated plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Serve {
    pub name: String,
    pub provider: String,
    pub build: Vec<Step>,
    pub deps: Vec<Package>,
    pub commands: IndexMap<String, String>,
    pub cwd: Option<String>,
    pub prepare: Option<Vec<RunStep>>,
    pub mounts: Option<Vec<Mount>>,
    pub volumes: Option<Vec<Volume>>,
    pub env: Option<IndexMap<String, String>>,
    pub services: Option<Vec<Service>>,
}
