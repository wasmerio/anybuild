use std::process::{Command, ExitStatus};
use std::sync::{Arc, Mutex, OnceLock};

use indexmap::IndexMap;

use crate::event::{Event, ProcessIo, ProcessStream, ProviderDetail, Reporter};

#[derive(Clone)]
pub(crate) struct OperationContext {
    environment: Arc<IndexMap<String, String>>,
    inherit_process_env: bool,
    process_io: ProcessIo,
    reporter: Reporter,
    reported_provider: Arc<OnceLock<String>>,
    secrets: Arc<Vec<String>>,
}

pub(crate) struct CapturedEvents(Arc<Mutex<Vec<Event>>>);

impl CapturedEvents {
    pub fn replay_into(self, operation: &OperationContext) {
        let events =
            std::mem::take(&mut *self.0.lock().expect("captured event lock is not poisoned"));
        for event in events {
            operation.emit(event);
        }
    }
}

impl OperationContext {
    pub fn new(
        environment: IndexMap<String, String>,
        inherit_process_env: bool,
        process_io: ProcessIo,
        reporter: Reporter,
    ) -> Self {
        let secrets = environment
            .iter()
            .filter(|(name, value)| is_sensitive_name(name) && !value.is_empty())
            .map(|(_, value)| value.clone())
            .collect();
        Self {
            environment: Arc::new(environment),
            inherit_process_env,
            process_io,
            reporter,
            reported_provider: Arc::new(OnceLock::new()),
            secrets: Arc::new(secrets),
        }
    }

    pub fn with_process_io(&self, process_io: ProcessIo) -> Self {
        Self {
            process_io,
            ..self.clone()
        }
    }

    pub fn without_environment(&self) -> Self {
        Self {
            environment: Arc::new(IndexMap::new()),
            inherit_process_env: false,
            ..self.clone()
        }
    }

    pub fn capture_events(&self) -> (Self, CapturedEvents) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        let context = Self {
            reporter: Reporter::new(move |event: &Event| {
                captured
                    .lock()
                    .expect("captured event lock is not poisoned")
                    .push(event.clone());
            }),
            ..self.clone()
        };
        (context, CapturedEvents(events))
    }

    pub fn emit(&self, event: Event) {
        self.reporter.emit(self.redact_event(event));
    }

    pub fn provider_detected(&self, provider: &str, details: Vec<ProviderDetail>) {
        if self.reported_provider.set(provider.to_owned()).is_err() {
            return;
        }
        self.emit(Event::ProviderDetected {
            provider: provider.to_owned(),
            details,
        });
    }

    pub fn environment_var(&self, name: &str) -> Option<String> {
        self.environment.get(name).cloned()
    }

    pub fn process_environment(&self) -> IndexMap<String, String> {
        self.environment.as_ref().clone()
    }

    pub fn prepare_command(&self, command: &mut Command) {
        if !self.inherit_process_env {
            command.env_clear();
        }
        command.envs(self.environment.iter());
    }

    pub fn command_status(&self, command: &mut Command) -> std::io::Result<ExitStatus> {
        match self.process_io {
            ProcessIo::Inherit => command.status(),
            ProcessIo::Events => {
                let output = command.output()?;
                if !output.stdout.is_empty() {
                    self.emit(Event::ProcessOutput {
                        stream: ProcessStream::Stdout,
                        text: String::from_utf8_lossy(&output.stdout).into_owned(),
                    });
                }
                if !output.stderr.is_empty() {
                    self.emit(Event::ProcessOutput {
                        stream: ProcessStream::Stderr,
                        text: String::from_utf8_lossy(&output.stderr).into_owned(),
                    });
                }
                Ok(output.status)
            }
        }
    }

    fn redact_event(&self, event: Event) -> Event {
        match event {
            Event::Diagnostic { level, message } => Event::Diagnostic {
                level,
                message: self.redact(message),
            },
            Event::ProviderDetected { provider, details } => Event::ProviderDetected {
                provider,
                details: details
                    .into_iter()
                    .map(|detail| ProviderDetail {
                        label: detail.label,
                        value: self.redact(detail.value),
                    })
                    .collect(),
            },
            Event::BuildStep { description } => Event::BuildStep {
                description: self.redact(description),
            },
            Event::CommandStarted { name, command } => Event::CommandStarted {
                name,
                command: command.map(|command| self.redact(command)),
            },
            Event::ProcessOutput { stream, text } => Event::ProcessOutput {
                stream,
                text: self.redact(text),
            },
            Event::Content { content, language } => Event::Content {
                content: self.redact(content),
                language,
            },
            Event::Deployment { description } => Event::Deployment {
                description: self.redact(description),
            },
            other => other,
        }
    }

    fn redact(&self, mut text: String) -> String {
        for secret in self.secrets.iter() {
            text = text.replace(secret, "[REDACTED]");
        }
        text
    }

    #[cfg(test)]
    pub fn for_test() -> Self {
        Self::new(
            std::env::vars().collect(),
            true,
            ProcessIo::Inherit,
            Reporter::default(),
        )
    }
}

fn is_sensitive_name(name: &str) -> bool {
    let name = name.to_ascii_uppercase();
    ["TOKEN", "PASSWORD", "SECRET", "CREDENTIAL", "API_KEY"]
        .iter()
        .any(|marker| name.contains(marker))
}
