//! Public Anybuild plan types.
//!
//! Plan-visible maps (env vars, commands) use `IndexMap`: Python dicts
//! preserve insertion order and plan JSON is the compatibility contract.

use std::fmt;
use std::path::PathBuf;

use indexmap::IndexMap;
use serde::Serialize;

pub(crate) mod layout;
#[cfg(test)]
pub(crate) mod snapshot;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Mount {
    pub name: String,
    #[serde(serialize_with = "serde_contract::path_lossy")]
    pub build_path: PathBuf,
    #[serde(serialize_with = "serde_contract::path_lossy")]
    pub serve_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Volume {
    pub name: String,
    #[serde(serialize_with = "serde_contract::path_lossy")]
    pub path: PathBuf,
    #[serde(serialize_with = "serde_contract::path_lossy")]
    pub serve_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Service {
    pub name: String,
    /// "postgres" | "mysql" | "redis"
    pub provider: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Package {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// "64-bit" | "32-bit"
    #[serde(skip_serializing_if = "Option::is_none")]
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

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunStep {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inputs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorkdirStep {
    #[serde(serialize_with = "serde_contract::path_lossy")]
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CopyStep {
    pub source: String,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore: Option<Vec<String>>,
    /// "source" | "assets"
    pub base: String,
}

impl CopyStep {
    pub fn is_download(&self) -> bool {
        self.source.starts_with("http://") || self.source.starts_with("https://")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EnvStep {
    pub variables: IndexMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UseStep {
    pub dependencies: Vec<Package>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PathStep {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WriteFileStep {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "__type__")]
pub enum Step {
    #[serde(rename = "RunStep")]
    Run(RunStep),
    #[serde(rename = "CopyStep")]
    Copy(CopyStep),
    #[serde(rename = "EnvStep")]
    Env(EnvStep),
    #[serde(rename = "PathStep")]
    Path(PathStep),
    #[serde(rename = "UseStep")]
    Use(UseStep),
    #[serde(rename = "WorkdirStep")]
    Workdir(WorkdirStep),
    #[serde(rename = "WriteFileStep")]
    WriteFile(WriteFileStep),
}

impl Step {
    /// The Python dataclass name used by the serialized `__type__` tag.
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
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Serve {
    pub name: String,
    pub provider: String,
    pub build: Vec<Step>,
    #[serde(serialize_with = "serde_contract::packages_as_strings")]
    pub deps: Vec<Package>,
    pub commands: IndexMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(serialize_with = "serde_contract::prepare_steps")]
    pub prepare: Option<Vec<RunStep>>,
    #[serde(serialize_with = "serde_contract::option_vec_as_empty")]
    pub mounts: Option<Vec<Mount>>,
    #[serde(serialize_with = "serde_contract::option_vec_as_empty")]
    pub volumes: Option<Vec<Volume>>,
    #[serde(serialize_with = "serde_contract::option_map_as_empty")]
    pub env: Option<IndexMap<String, String>>,
    #[serde(serialize_with = "serde_contract::option_vec_as_empty")]
    pub services: Option<Vec<Service>>,
}

mod serde_contract {
    use std::path::Path;

    use indexmap::IndexMap;
    use serde::{Serialize, Serializer};

    use crate::plan::{Package, RunStep};

    pub fn path_lossy<S>(path: &Path, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&path.to_string_lossy())
    }

    pub fn packages_as_strings<S>(packages: &[Package], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_seq(packages.iter().map(ToString::to_string))
    }

    pub fn option_vec_as_empty<T, S>(
        value: &Option<Vec<T>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        T: Serialize,
        S: Serializer,
    {
        value.as_deref().unwrap_or_default().serialize(serializer)
    }

    pub fn option_map_as_empty<S>(
        value: &Option<IndexMap<String, String>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(value) => value.serialize(serializer),
            None => IndexMap::<String, String>::new().serialize(serializer),
        }
    }

    #[derive(Serialize)]
    struct TaggedRunStep<'a> {
        #[serde(rename = "__type__")]
        type_name: &'static str,
        #[serde(flatten)]
        step: &'a RunStep,
    }

    pub fn prepare_steps<S>(value: &Option<Vec<RunStep>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_seq(value.as_deref().unwrap_or_default().iter().map(|step| {
            TaggedRunStep {
                type_name: "RunStep",
                step,
            }
        }))
    }
}
