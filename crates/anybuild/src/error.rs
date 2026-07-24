use std::path::PathBuf;

/// Broad error category suitable for programmatic handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    InvalidInput,
    Project,
    Generation,
    Evaluation,
    Build,
    Run,
    Deploy,
    Io,
}

/// SDK error with operation and project context.
#[derive(Debug, thiserror::Error)]
#[error("{operation} failed for {}: {source:#}", path.display())]
pub struct Error {
    kind: ErrorKind,
    operation: &'static str,
    path: PathBuf,
    #[source]
    source: anyhow::Error,
}

impl Error {
    pub(crate) fn new(
        kind: ErrorKind,
        operation: &'static str,
        path: impl Into<PathBuf>,
        source: impl Into<anyhow::Error>,
    ) -> Self {
        Self {
            kind,
            operation,
            path: path.into(),
            source: source.into(),
        }
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn operation(&self) -> &'static str {
        self.operation
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

pub type Result<T> = std::result::Result<T, Error>;
