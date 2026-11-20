use std::fmt;

use anyhow::{bail, ensure, Context, Result};
use serde::Serialize;

/// Selects how a package is resolved for a builder or runtime.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyBackend {
    #[default]
    Default,
    Mise,
    Wasmer,
}

impl DependencyBackend {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Mise => "mise",
            Self::Wasmer => "wasmer",
        }
    }

    fn is_default(&self) -> bool {
        *self == Self::Default
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Package {
    /// The package identifier within its backend.
    pub name: String,
    #[serde(skip_serializing_if = "DependencyBackend::is_default")]
    pub backend: DependencyBackend,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// "64-bit" | "32-bit"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architecture: Option<String>,
}

impl Package {
    /// Parse a `dep()` name into a backend and a backend-specific package name.
    pub fn parse(
        reference: &str,
        version: Option<&str>,
        architecture: Option<&str>,
    ) -> Result<Self> {
        let (backend, name) = match reference.split_once(':') {
            Some(("mise", name)) => (DependencyBackend::Mise, name),
            Some(("wasmer", name)) => (DependencyBackend::Wasmer, name),
            Some((backend, _)) => bail!("Unknown dependency backend {backend:?} in {reference}"),
            None => (DependencyBackend::Default, reference),
        };
        match backend {
            DependencyBackend::Default => {}
            DependencyBackend::Mise => {
                ensure!(
                    !name.is_empty()
                        && !name.starts_with(['-', ':'])
                        && !name.ends_with([':', '@'])
                        && !name.contains(|c: char| c.is_whitespace() || c.is_control()),
                    "Invalid mise tool identifier in dependency {reference:?}"
                );
                if let Some(version) = version {
                    ensure!(
                        !version.is_empty()
                            && !version.contains(|c: char| c.is_whitespace() || c.is_control()),
                        "Invalid mise version {version:?} in dependency {reference}"
                    );
                }
            }
            DependencyBackend::Wasmer => {
                ensure!(
                    name.split_once('/').is_some_and(|(namespace, name)| {
                        valid_wasmer_name_part(namespace) && valid_wasmer_name_part(name)
                    }),
                    "Invalid Wasmer package name in dependency {reference}. Expected wasmer:namespace/name"
                );
                if let Some(version) = version {
                    semver::VersionReq::parse(version).with_context(|| {
                        format!("Invalid Wasmer version requirement in {reference}@{version}")
                    })?;
                }
            }
        }
        ensure!(
            backend.is_default() || architecture.is_none(),
            "Dependency {reference} does not support the architecture argument"
        );
        Ok(Self {
            name: name.to_owned(),
            backend,
            version: version.map(str::to_owned),
            architecture: architecture.map(str::to_owned),
        })
    }

    pub(crate) fn is_default_named(&self, name: &str) -> bool {
        self.backend.is_default() && self.name == name
    }

    pub(crate) fn mise_tool_spec(&self) -> String {
        let version = self.version.as_deref().unwrap_or("latest");
        format!("{}@{version}", self.name)
    }

    pub(crate) fn qualified_name(&self) -> String {
        if self.backend.is_default() {
            self.name.clone()
        } else {
            format!("{}:{}", self.backend.as_str(), self.name)
        }
    }
}

impl fmt::Display for Package {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.backend.is_default() {
            write!(f, "{}:", self.backend.as_str())?;
        }
        f.write_str(&self.name)?;
        if let Some(arch) = &self.architecture {
            write!(f, "({arch})")?;
        }
        if let Some(version) = &self.version {
            write!(f, "@{version}")?;
        }
        Ok(())
    }
}

fn valid_wasmer_name_part(part: &str) -> bool {
    !matches!(part, "" | "." | "..")
        && part
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_qualified_dependencies_are_rejected() {
        for (name, version, architecture) in [
            ("wasmer:foo", None, None),
            ("wasmer:foo/../bar", None, None),
            ("wasmer:foo/bar", Some("latest"), None),
            ("wasmer:foo/bar", None, Some("64-bit")),
            ("mise:", None, None),
            ("mise:--global", None, None),
            ("mise:aqua:", None, None),
            ("mise:node@", None, None),
            ("mise:node", Some("24\n25"), None),
            ("mies:node", None, None),
        ] {
            assert!(
                Package::parse(name, version, architecture).is_err(),
                "{name}"
            );
        }
    }
}
