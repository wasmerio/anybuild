//! Build step types

use allocative::Allocative;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

/// A run step - executes a command
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Allocative)]
pub struct RunStep {
    /// Command to execute
    pub command: String,
    /// Input files/directories that this step depends on
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inputs: Option<Vec<String>>,
    /// Output files/directories that this step produces
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Vec<String>>,
    /// Optional group name for organizing steps
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

impl RunStep {
    /// Create a new run step
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            inputs: None,
            outputs: None,
            group: None,
        }
    }

    /// Set inputs
    pub fn with_inputs(mut self, inputs: Vec<String>) -> Self {
        self.inputs = Some(inputs);
        self
    }

    /// Set outputs
    pub fn with_outputs(mut self, outputs: Vec<String>) -> Self {
        self.outputs = Some(outputs);
        self
    }

    /// Set group
    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }
}

/// A workdir step - changes the working directory
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Allocative)]
pub struct WorkdirStep {
    /// Path to set as working directory
    pub path: PathBuf,
}

impl WorkdirStep {
    /// Create a new workdir step
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

/// Base for copy operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Allocative)]
#[serde(rename_all = "lowercase")]
pub enum CopyBase {
    Source,
    Assets,
}

/// A copy step - copies files or downloads from URL
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Allocative)]
pub struct CopyStep {
    /// Source path or URL
    pub source: String,
    /// Target path
    pub target: String,
    /// Patterns to ignore during copy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore: Option<Vec<String>>,
    /// Base directory for relative paths
    #[serde(default = "default_copy_base")]
    pub base: CopyBase,
}

fn default_copy_base() -> CopyBase {
    CopyBase::Source
}

impl CopyStep {
    /// Create a new copy step
    pub fn new(source: impl Into<String>, target: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            target: target.into(),
            ignore: None,
            base: CopyBase::Source,
        }
    }

    /// Set ignore patterns
    pub fn with_ignore(mut self, ignore: Vec<String>) -> Self {
        self.ignore = Some(ignore);
        self
    }

    /// Set base directory
    pub fn with_base(mut self, base: CopyBase) -> Self {
        self.base = base;
        self
    }

    /// Check if this is a download operation (HTTP/HTTPS URL)
    pub fn is_download(&self) -> bool {
        self.source.starts_with("http://") || self.source.starts_with("https://")
    }
}

/// An env step - sets environment variables
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Allocative)]
pub struct EnvStep {
    /// Environment variables to set
    pub variables: HashMap<String, String>,
}

impl EnvStep {
    /// Create a new env step
    pub fn new(variables: HashMap<String, String>) -> Self {
        Self { variables }
    }
}

impl fmt::Display for EnvStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let vars: Vec<String> = self
            .variables
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        write!(f, "{}", vars.join(" "))
    }
}

/// A use step - declares package dependencies
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Allocative)]
pub struct UseStep {
    /// Package dependency references (format: "ref:package:name")
    pub dependencies: Vec<String>,
}

impl UseStep {
    /// Create a new use step
    pub fn new(dependencies: Vec<String>) -> Self {
        Self { dependencies }
    }
}

/// A path step - adds to PATH environment variable
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Allocative)]
pub struct PathStep {
    /// Path to add
    pub path: String,
}

impl PathStep {
    /// Create a new path step
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }
}

/// Union of all step types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Allocative)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Step {
    Run(RunStep),
    Copy(CopyStep),
    Env(EnvStep),
    Path(PathStep),
    Use(UseStep),
    Workdir(WorkdirStep),
}

// Convenience From implementations
impl From<RunStep> for Step {
    fn from(step: RunStep) -> Self {
        Step::Run(step)
    }
}

impl From<CopyStep> for Step {
    fn from(step: CopyStep) -> Self {
        Step::Copy(step)
    }
}

impl From<EnvStep> for Step {
    fn from(step: EnvStep) -> Self {
        Step::Env(step)
    }
}

impl From<PathStep> for Step {
    fn from(step: PathStep) -> Self {
        Step::Path(step)
    }
}

impl From<UseStep> for Step {
    fn from(step: UseStep) -> Self {
        Step::Use(step)
    }
}

impl From<WorkdirStep> for Step {
    fn from(step: WorkdirStep) -> Self {
        Step::Workdir(step)
    }
}

/// Prepare step type (currently only RunStep)
pub type PrepareStep = RunStep;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_step() {
        let step = RunStep::new("npm install");
        assert_eq!(step.command, "npm install");
        assert!(step.inputs.is_none());
        assert!(step.outputs.is_none());
    }

    #[test]
    fn test_run_step_builder() {
        let step = RunStep::new("npm run build")
            .with_inputs(vec!["src/".to_string()])
            .with_outputs(vec!["dist/".to_string()])
            .with_group("build");

        assert_eq!(step.inputs, Some(vec!["src/".to_string()]));
        assert_eq!(step.outputs, Some(vec!["dist/".to_string()]));
        assert_eq!(step.group, Some("build".to_string()));
    }

    #[test]
    fn test_copy_step_download() {
        let step = CopyStep::new("https://example.com/file.tar.gz", "/tmp/file.tar.gz");
        assert!(step.is_download());

        let step = CopyStep::new("./src", "./dest");
        assert!(!step.is_download());
    }

    #[test]
    fn test_env_step_display() {
        let mut vars = HashMap::new();
        vars.insert("NODE_ENV".to_string(), "production".to_string());
        vars.insert("PORT".to_string(), "3000".to_string());

        let step = EnvStep::new(vars);
        let display = step.to_string();
        assert!(display.contains("NODE_ENV=production"));
        assert!(display.contains("PORT=3000"));
    }

    #[test]
    fn test_workdir_step() {
        let step = WorkdirStep::new("/app");
        assert_eq!(step.path, PathBuf::from("/app"));
    }

    #[test]
    fn test_path_step() {
        let step = PathStep::new("/usr/local/bin");
        assert_eq!(step.path, "/usr/local/bin");
    }

    #[test]
    fn test_use_step() {
        let step = UseStep::new(vec![
            "ref:package:python".to_string(),
            "ref:package:node".to_string(),
        ]);
        assert_eq!(step.dependencies.len(), 2);
    }

    #[test]
    fn test_step_enum() {
        let step: Step = RunStep::new("echo hello").into();
        assert!(matches!(step, Step::Run(_)));

        let step: Step = CopyStep::new("./src", "./dest").into();
        assert!(matches!(step, Step::Copy(_)));
    }

    #[test]
    fn test_step_serialization() {
        let step = Step::Run(RunStep::new("npm install"));
        let json = serde_json::to_string(&step).unwrap();
        let deserialized: Step = serde_json::from_str(&json).unwrap();
        assert_eq!(step, deserialized);
    }
}
