use indexmap::IndexMap;
use serde::Serialize;
use std::cell::RefCell;
use std::path::PathBuf;
use std::process::{Command, ExitStatus};
use std::sync::Arc;

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

thread_local! {
    static CURRENT: RefCell<Reporter> = RefCell::new(Reporter::default());
    static PROCESS_IO: RefCell<ProcessIo> = const { RefCell::new(ProcessIo::Inherit) };
    static PROCESS_ENV: RefCell<Option<(IndexMap<String, String>, bool)>> =
        const { RefCell::new(None) };
    static SECRET_VALUES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

#[doc(hidden)]
pub fn scope_process_environment<T>(
    environment: &IndexMap<String, String>,
    inherit: bool,
    operation: impl FnOnce() -> T,
) -> T {
    PROCESS_ENV.with(|current| {
        let previous = current.replace(Some((environment.clone(), inherit)));
        let secrets = environment
            .iter()
            .filter(|(name, value)| is_sensitive_name(name) && !value.is_empty())
            .map(|(_, value)| value.clone())
            .collect();
        let previous_secrets = SECRET_VALUES.with(|current| current.replace(secrets));
        let result = operation();
        SECRET_VALUES.with(|current| current.replace(previous_secrets));
        current.replace(previous);
        result
    })
}

fn is_sensitive_name(name: &str) -> bool {
    let name = name.to_ascii_uppercase();
    ["TOKEN", "PASSWORD", "SECRET", "CREDENTIAL", "API_KEY"]
        .iter()
        .any(|marker| name.contains(marker))
}

fn redact_text(mut text: String) -> String {
    SECRET_VALUES.with(|values| {
        for value in values.borrow().iter() {
            text = text.replace(value, "[REDACTED]");
        }
    });
    text
}

fn redact_event(event: Event) -> Event {
    match event {
        Event::Diagnostic { level, message } => Event::Diagnostic {
            level,
            message: redact_text(message),
        },
        Event::BuildStep { description } => Event::BuildStep {
            description: redact_text(description),
        },
        Event::CommandStarted { name, command } => Event::CommandStarted {
            name,
            command: command.map(redact_text),
        },
        Event::ProcessOutput { stream, text } => Event::ProcessOutput {
            stream,
            text: redact_text(text),
        },
        Event::Content { content, language } => Event::Content {
            content: redact_text(content),
            language,
        },
        Event::Deployment { description } => Event::Deployment {
            description: redact_text(description),
        },
        other => other,
    }
}

#[doc(hidden)]
pub fn prepare_command(command: &mut Command) {
    PROCESS_ENV.with(|current| {
        if let Some((environment, inherit)) = current.borrow().as_ref() {
            if !inherit {
                command.env_clear();
            }
            command.envs(environment);
        }
    });
}

#[doc(hidden)]
pub fn process_environment() -> IndexMap<String, String> {
    PROCESS_ENV.with(|current| {
        current
            .borrow()
            .as_ref()
            .map(|(environment, _)| environment.clone())
            .unwrap_or_else(|| std::env::vars().collect())
    })
}

#[doc(hidden)]
pub fn environment_var(name: &str) -> Option<String> {
    PROCESS_ENV.with(|current| match current.borrow().as_ref() {
        Some((environment, _)) => environment.get(name).cloned(),
        None => std::env::var(name).ok(),
    })
}

#[doc(hidden)]
pub fn emit(event: Event) {
    CURRENT.with(|current| current.borrow().emit(redact_event(event)));
}

#[doc(hidden)]
pub fn scope<T>(reporter: &Reporter, operation: impl FnOnce() -> T) -> T {
    CURRENT.with(|current| {
        let previous = current.replace(reporter.clone());
        let result = operation();
        current.replace(previous);
        result
    })
}

#[doc(hidden)]
pub fn scope_process_io<T>(mode: ProcessIo, operation: impl FnOnce() -> T) -> T {
    PROCESS_IO.with(|current| {
        let previous = current.replace(mode);
        let result = operation();
        current.replace(previous);
        result
    })
}

#[doc(hidden)]
pub fn command_status(command: &mut Command) -> std::io::Result<ExitStatus> {
    let mode = PROCESS_IO.with(|current| *current.borrow());
    match mode {
        ProcessIo::Inherit => command.status(),
        ProcessIo::Events => {
            let output = command.output()?;
            if !output.stdout.is_empty() {
                emit(Event::ProcessOutput {
                    stream: ProcessStream::Stdout,
                    text: String::from_utf8_lossy(&output.stdout).into_owned(),
                });
            }
            if !output.stderr.is_empty() {
                emit(Event::ProcessOutput {
                    stream: ProcessStream::Stderr,
                    text: String::from_utf8_lossy(&output.stderr).into_owned(),
                });
            }
            Ok(output.status)
        }
    }
}
