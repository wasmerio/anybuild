use anyhow::{bail, ensure, Context, Result};

use crate::plan::{Package, Step};

#[derive(Clone, Copy)]
pub(crate) enum DependencyBackend<'a> {
    Default,
    Mise(&'a str),
    Wasmer(&'a str),
}

impl<'a> DependencyBackend<'a> {
    pub(crate) fn parse(package: &'a Package) -> Result<Self> {
        let Some((backend, name)) = package.name.split_once(':') else {
            return Ok(Self::Default);
        };
        let backend = match backend {
            "mise" => {
                ensure!(
                    !name.is_empty()
                        && !name.starts_with(['-', ':'])
                        && !name.ends_with([':', '@'])
                        && !name.contains(|c: char| c.is_whitespace() || c.is_control()),
                    "Invalid mise tool identifier in dependency {package:?}"
                );
                if let Some(version) = &package.version {
                    ensure!(
                        !version.is_empty()
                            && !version.contains(|c: char| c.is_whitespace() || c.is_control()),
                        "Invalid mise version in dependency {package}"
                    );
                }
                Self::Mise(name)
            }
            "wasmer" => {
                ensure!(
                    name.split_once('/').is_some_and(|(namespace, name)| {
                        valid_wasmer_name_part(namespace) && valid_wasmer_name_part(name)
                    }),
                    "Invalid Wasmer package name in dependency {package}. Expected wasmer:namespace/name"
                );
                if let Some(version) = &package.version {
                    semver::VersionReq::parse(version).with_context(|| {
                        format!("Invalid Wasmer version requirement in {package}")
                    })?;
                }
                Self::Wasmer(name)
            }
            _ => bail!("Unknown dependency backend {backend:?} in {package}"),
        };
        ensure!(
            package.architecture.is_none(),
            "Dependency {package} does not support the architecture argument"
        );
        Ok(backend)
    }

    pub(crate) fn for_build(package: &'a Package) -> Result<Self> {
        let backend = Self::parse(package)?;
        ensure!(
            !matches!(backend, Self::Wasmer(_)),
            "Build dependency {package} requires serve_deps with the Wasmer runner"
        );
        Ok(backend)
    }
}

fn valid_wasmer_name_part(part: &str) -> bool {
    !matches!(part, "" | "." | "..")
        && part
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
}

pub(crate) fn mise_tool_spec(tool: &str, package: &Package) -> String {
    let version = package.version.as_deref().unwrap_or("latest");
    format!("{tool}@{version}")
}

/// Returns whether the build must install mise tools, even without run steps.
pub(crate) fn validate_build_dependencies(steps: &[Step]) -> Result<bool> {
    let mut installs_tools = false;
    for step in steps {
        if let Step::Use(step) = step {
            for package in &step.dependencies {
                installs_tools |= matches!(
                    DependencyBackend::for_build(package)?,
                    DependencyBackend::Mise(_)
                );
            }
        }
    }
    Ok(installs_tools)
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
            let package = Package {
                name: name.to_owned(),
                version: version.map(str::to_owned),
                architecture: architecture.map(str::to_owned),
            };
            assert!(DependencyBackend::parse(&package).is_err(), "{package}");
        }
    }
}
