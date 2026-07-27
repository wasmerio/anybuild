//! Structured events emitted by SDK operations.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DiagnosticLevel {
    Info,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderDetail {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PackagePhase {
    Build,
    Deploy,
    Both,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildPlanPackage {
    pub name: String,
    pub version: Option<String>,
    pub architecture: Option<String>,
    pub phase: PackagePhase,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BuildPlanStep {
    Run {
        command: String,
        group: Option<String>,
    },
    Copy {
        source: String,
        target: String,
        base: String,
    },
    Environment {
        variables: Vec<String>,
    },
    Path {
        path: String,
    },
    Workdir {
        path: PathBuf,
    },
    WriteFile {
        path: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeployScript {
    pub name: String,
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WasmerPackageMapping {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Event {
    Diagnostic {
        level: DiagnosticLevel,
        message: String,
    },
    LegacyRenamed {
        from: PathBuf,
        to: PathBuf,
    },
    ProviderDetected {
        provider: String,
        details: Vec<ProviderDetail>,
    },
    ProviderDeclared {
        provider: String,
        details: Vec<ProviderDetail>,
    },
    AnybuildGenerating {
        path: PathBuf,
        provider: String,
        config: serde_json::Value,
    },
    BuildPlan {
        packages: Vec<BuildPlanPackage>,
        steps: Vec<BuildPlanStep>,
        prepare_steps: Vec<BuildPlanStep>,
        deploy_scripts: Vec<DeployScript>,
    },
    FileWritten {
        kind: &'static str,
        path: PathBuf,
    },
    SectionStarted {
        title: String,
    },
    WasmerPackageMappings {
        mappings: Vec<WasmerPackageMapping>,
    },
    Success {
        message: String,
    },
    WasmerFileContent {
        filename: String,
        content: String,
        language: String,
    },
    BuildStarted,
    BuildStep {
        description: String,
    },
    CommandStarted {
        name: String,
        command: Option<String>,
    },
    ProcessOutput {
        stream: ProcessStream,
        text: String,
    },
    Content {
        content: String,
        language: Option<String>,
    },
    ArtifactCreated {
        path: PathBuf,
    },
    Deployment {
        description: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ProcessStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum ProcessIo {
    #[default]
    Inherit,
    Events,
}

pub trait EventHandler: Send + Sync {
    fn on_event(&self, event: &Event);
}

impl<F> EventHandler for F
where
    F: Fn(&Event) + Send + Sync,
{
    fn on_event(&self, event: &Event) {
        self(event);
    }
}

#[doc(hidden)]
#[derive(Clone, Default)]
pub struct Reporter(Option<Arc<dyn EventHandler>>);

impl Reporter {
    pub fn new(handler: impl EventHandler + 'static) -> Self {
        Self(Some(Arc::new(handler)))
    }

    pub fn emit(&self, event: Event) {
        if let Some(handler) = &self.0 {
            handler.on_event(&event);
        }
    }
}
