//! Runtime artifacts produced by runners.

use std::path::PathBuf;

use anyhow::{Context, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

const ARTIFACT_MANIFEST: &str = "artifact.json";

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Local,
    Docker,
    LambdaZip,
    Wasmer,
    Collection,
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
    LambdaZip {
        archive: PathBuf,
        runtime: String,
        handler: String,
        #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
        environment: IndexMap<String, String>,
        #[serde(default)]
        platform: Option<String>,
    },
    Wasmer {
        directory: PathBuf,
    },
    Collection {
        artifacts: Vec<RuntimeArtifact>,
    },
}

impl RuntimeArtifact {
    pub fn kind(&self) -> ArtifactKind {
        match self {
            Self::Local { .. } => ArtifactKind::Local,
            Self::Docker { .. } => ArtifactKind::Docker,
            Self::LambdaZip { .. } => ArtifactKind::LambdaZip,
            Self::Wasmer { .. } => ArtifactKind::Wasmer,
            Self::Collection { .. } => ArtifactKind::Collection,
        }
    }

    pub(crate) fn contains_kind(&self, kind: ArtifactKind) -> bool {
        self.find(kind).is_some()
    }

    fn find(&self, kind: ArtifactKind) -> Option<&Self> {
        if self.kind() == kind {
            return Some(self);
        }
        match self {
            Self::Collection { artifacts } => artifacts.iter().find_map(|item| item.find(kind)),
            _ => None,
        }
    }

    pub(crate) fn wasmer_directory(&self) -> Option<&std::path::Path> {
        match self {
            Self::Wasmer { directory } => Some(directory),
            Self::Collection { artifacts } => {
                artifacts.iter().find_map(RuntimeArtifact::wasmer_directory)
            }
            Self::Local { .. } | Self::Docker { .. } | Self::LambdaZip { .. } => None,
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
            Self::Collection { artifacts } => {
                artifacts.iter().find_map(RuntimeArtifact::docker_parts)
            }
            Self::Local { .. } | Self::LambdaZip { .. } | Self::Wasmer { .. } => None,
        }
    }

    pub(crate) fn platform(&self) -> Option<&str> {
        match self {
            Self::Docker { platform, .. } | Self::LambdaZip { platform, .. } => platform.as_deref(),
            Self::Collection { artifacts } => artifacts.iter().find_map(RuntimeArtifact::platform),
            Self::Local { .. } | Self::Wasmer { .. } => None,
        }
    }

    pub(crate) fn lambda_zip(&self) -> Option<&Self> {
        self.find(ArtifactKind::LambdaZip)
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
        let lambda = RuntimeArtifact::LambdaZip {
            archive: temporary.path().join("function.zip"),
            runtime: "python3.13".to_owned(),
            handler: "run.sh".to_owned(),
            environment: IndexMap::from([(
                "AWS_LAMBDA_EXEC_WRAPPER".to_owned(),
                "/opt/bootstrap".to_owned(),
            )]),
            platform: Some("linux/amd64".to_owned()),
        };
        let artifact = RuntimeArtifact::Collection {
            artifacts: vec![artifact, lambda],
        };

        artifact.persist(temporary.path()).unwrap();
        let loaded = RuntimeArtifact::load(temporary.path()).unwrap().unwrap();

        assert_eq!(loaded.kind(), ArtifactKind::Collection);
        assert!(loaded.contains_kind(ArtifactKind::Docker));
        assert!(matches!(
            loaded.lambda_zip(),
            Some(RuntimeArtifact::LambdaZip { runtime, .. }) if runtime == "python3.13"
        ));
        assert_eq!(loaded.docker_parts().unwrap().1, "acme-api");
    }
}
