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
    },
    FileWritten {
        kind: &'static str,
        path: PathBuf,
    },
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
