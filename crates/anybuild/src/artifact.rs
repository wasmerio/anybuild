//! Runtime artifacts produced by runners.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const ARTIFACT_MANIFEST: &str = "artifact.json";

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Local,
    Docker,
    Wasmer,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeArtifact {
    Local {
        directory: PathBuf,
    },
    Docker {
        directory: PathBuf,
        image: String,
        context: PathBuf,
        #[serde(default)]
        platform: Option<String>,
    },
    Wasmer {
        directory: PathBuf,
    },
}

impl RuntimeArtifact {
    pub fn kind(&self) -> ArtifactKind {
        match self {
            Self::Local { .. } => ArtifactKind::Local,
            Self::Docker { .. } => ArtifactKind::Docker,
            Self::Wasmer { .. } => ArtifactKind::Wasmer,
        }
    }

    pub(crate) fn wasmer_directory(&self) -> Option<&std::path::Path> {
        match self {
            Self::Wasmer { directory } => Some(directory),
            Self::Local { .. } | Self::Docker { .. } => None,
        }
    }

    pub(crate) fn docker_parts(&self) -> Option<(&std::path::Path, &str, &std::path::Path)> {
        match self {
            Self::Docker {
                directory,
                image,
                context,
                ..
            } => Some((directory, image, context)),
            Self::Local { .. } | Self::Wasmer { .. } => None,
        }
    }

    pub(crate) fn docker_platform(&self) -> Option<&str> {
        match self {
            Self::Docker { platform, .. } => platform.as_deref(),
            Self::Local { .. } | Self::Wasmer { .. } => None,
        }
    }

    pub(crate) fn persist(&self, anybuild_dir: &std::path::Path) -> Result<()> {
        std::fs::create_dir_all(anybuild_dir)?;
        let manifest = serde_json::to_string_pretty(self)?;
        std::fs::write(
            anybuild_dir.join(ARTIFACT_MANIFEST),
            format!("{manifest}\n"),
        )?;
        Ok(())
    }

    pub(crate) fn load(anybuild_dir: &std::path::Path) -> Result<Option<Self>> {
        let path = anybuild_dir.join(ARTIFACT_MANIFEST);
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        serde_json::from_str(&contents)
            .with_context(|| format!("invalid runtime artifact manifest {}", path.display()))
            .map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_manifest_round_trips() {
        let temporary = tempfile::tempdir().unwrap();
        let artifact = RuntimeArtifact::Docker {
            directory: temporary.path().join("docker"),
            image: "acme-api".to_owned(),
            context: temporary.path().join("project"),
            platform: Some("linux/amd64".to_owned()),
        };

        artifact.persist(temporary.path()).unwrap();
        let loaded = RuntimeArtifact::load(temporary.path()).unwrap().unwrap();

        assert_eq!(loaded.kind(), ArtifactKind::Docker);
        assert!(matches!(
            loaded,
            RuntimeArtifact::Docker { image, .. } if image == "acme-api"
        ));
    }
}
