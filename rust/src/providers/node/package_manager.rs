//! Node.js package manager detection and utilities.
//!
//! This module provides functionality to detect which package manager a Node.js
//! project uses (npm, pnpm, yarn, or bun) and generate appropriate commands.

use crate::providers::specs::DependencySpec;
use std::path::Path;

/// Supported Node.js package managers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl PackageManager {
    /// Returns the lockfile name for this package manager.
    pub fn lockfile(&self) -> &str {
        match self {
            Self::Npm => "package-lock.json",
            Self::Pnpm => "pnpm-lock.yaml",
            Self::Yarn => "yarn.lock",
            Self::Bun => "bun.lockb",
        }
    }

    /// Returns the install command for this package manager.
    ///
    /// If `has_lockfile` is true, uses ci/frozen-lockfile for deterministic
    /// installs.
    pub fn install_command(&self, has_lockfile: bool) -> String {
        match (self, has_lockfile) {
            (Self::Npm, true) => "npm ci".to_string(),
            (Self::Npm, false) => "npm install".to_string(),
            (Self::Pnpm, true) => "pnpm install --frozen-lockfile".to_string(),
            (Self::Pnpm, false) => "pnpm install".to_string(),
            (Self::Yarn, true) => "yarn install --frozen-lockfile".to_string(),
            (Self::Yarn, false) => "yarn install".to_string(),
            (Self::Bun, _) => "bun install".to_string(),
        }
    }

    /// Returns the run command for this package manager.
    pub fn run_command(&self, cmd: &str) -> String {
        match self {
            Self::Npm => format!("npm run {}", cmd),
            Self::Pnpm => format!("pnpm run {}", cmd),
            Self::Yarn => format!("yarn run {}", cmd),
            Self::Bun => format!("bun run {}", cmd),
        }
    }

    /// Returns the execute command for this package manager (npx, pnpx, etc.).
    pub fn run_execute_command(&self, cmd: &str) -> String {
        match self {
            Self::Npm => format!("npx {}", cmd),
            Self::Pnpm => format!("pnpx {}", cmd),
            Self::Yarn => format!("yarn dlx {}", cmd),
            Self::Bun => format!("bunx {}", cmd),
        }
    }

    /// Returns the dependency spec for this package manager.
    pub fn as_dependency(&self) -> DependencySpec {
        match self {
            Self::Npm => DependencySpec::new("node"),
            Self::Pnpm => {
                let mut spec = DependencySpec::new("pnpm");
                spec.use_in_build = true;
                spec
            }
            Self::Yarn => {
                let mut spec = DependencySpec::new("yarn");
                spec.use_in_build = true;
                spec
            }
            Self::Bun => {
                let mut spec = DependencySpec::new("bun");
                spec.use_in_build = true;
                spec
            }
        }
    }
}

/// Detects which package manager a Node.js project uses.
///
/// Detection is based on lockfile presence in priority order:
/// 1. bun.lockb → Bun
/// 2. pnpm-lock.yaml → pnpm
/// 3. yarn.lock → Yarn
/// 4. package-lock.json → npm
/// 5. Defaults to npm if package.json exists but no lockfile
pub fn detect_package_manager(path: &Path) -> Option<PackageManager> {
    if !path.join("package.json").exists() {
        return None;
    }

    // Check lockfiles in priority order
    if path.join("bun.lockb").exists() {
        Some(PackageManager::Bun)
    } else if path.join("pnpm-lock.yaml").exists() {
        Some(PackageManager::Pnpm)
    } else if path.join("yarn.lock").exists() {
        Some(PackageManager::Yarn)
    } else if path.join("package-lock.json").exists() {
        Some(PackageManager::Npm)
    } else {
        // Default to npm if package.json exists
        Some(PackageManager::Npm)
    }
}

/// Detects the pnpm version from pnpm-lock.yaml.
///
/// Maps lockfileVersion to pnpm version:
/// - "5.x" → pnpm 7
/// - "6.x" → pnpm 8
/// - "7.x" → pnpm 9
pub fn detect_pnpm_version(lockfile: &Path) -> Option<String> {
    let content = std::fs::read_to_string(lockfile).ok()?;

    // Parse YAML to find lockfileVersion
    for line in content.lines() {
        if line.starts_with("lockfileVersion:") {
            let version = line
                .split(':')
                .nth(1)?
                .trim()
                .trim_matches('\'')
                .trim_matches('"');

            // Map lockfile version to pnpm version
            if version.starts_with("5.") {
                return Some("7".to_string());
            } else if version.starts_with("6.") {
                return Some("8".to_string());
            } else if version.starts_with("7.") {
                return Some("9".to_string());
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_lockfile_names() {
        assert_eq!(PackageManager::Npm.lockfile(), "package-lock.json");
        assert_eq!(PackageManager::Pnpm.lockfile(), "pnpm-lock.yaml");
        assert_eq!(PackageManager::Yarn.lockfile(), "yarn.lock");
        assert_eq!(PackageManager::Bun.lockfile(), "bun.lockb");
    }

    #[test]
    fn test_install_commands() {
        assert_eq!(PackageManager::Npm.install_command(true), "npm ci");
        assert_eq!(PackageManager::Npm.install_command(false), "npm install");
        assert_eq!(
            PackageManager::Pnpm.install_command(true),
            "pnpm install --frozen-lockfile"
        );
        assert_eq!(PackageManager::Yarn.install_command(false), "yarn install");
        assert_eq!(PackageManager::Bun.install_command(true), "bun install");
    }

    #[test]
    fn test_run_commands() {
        assert_eq!(PackageManager::Npm.run_command("build"), "npm run build");
        assert_eq!(PackageManager::Pnpm.run_command("dev"), "pnpm run dev");
        assert_eq!(PackageManager::Yarn.run_command("test"), "yarn run test");
        assert_eq!(PackageManager::Bun.run_command("start"), "bun run start");
    }

    #[test]
    fn test_execute_commands() {
        assert_eq!(
            PackageManager::Npm.run_execute_command("eslint"),
            "npx eslint"
        );
        assert_eq!(
            PackageManager::Pnpm.run_execute_command("prettier"),
            "pnpx prettier"
        );
        assert_eq!(
            PackageManager::Yarn.run_execute_command("tsc"),
            "yarn dlx tsc"
        );
        assert_eq!(PackageManager::Bun.run_execute_command("vite"), "bunx vite");
    }

    #[test]
    fn test_detect_bun() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        fs::write(path.join("package.json"), "{}").unwrap();
        fs::write(path.join("bun.lockb"), "").unwrap();

        assert_eq!(detect_package_manager(path), Some(PackageManager::Bun));
    }

    #[test]
    fn test_detect_pnpm() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        fs::write(path.join("package.json"), "{}").unwrap();
        fs::write(path.join("pnpm-lock.yaml"), "").unwrap();

        assert_eq!(detect_package_manager(path), Some(PackageManager::Pnpm));
    }

    #[test]
    fn test_detect_yarn() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        fs::write(path.join("package.json"), "{}").unwrap();
        fs::write(path.join("yarn.lock"), "").unwrap();

        assert_eq!(detect_package_manager(path), Some(PackageManager::Yarn));
    }

    #[test]
    fn test_detect_npm() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        fs::write(path.join("package.json"), "{}").unwrap();
        fs::write(path.join("package-lock.json"), "").unwrap();

        assert_eq!(detect_package_manager(path), Some(PackageManager::Npm));
    }

    #[test]
    fn test_detect_npm_default() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        fs::write(path.join("package.json"), "{}").unwrap();

        assert_eq!(detect_package_manager(path), Some(PackageManager::Npm));
    }

    #[test]
    fn test_detect_no_package_json() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        assert_eq!(detect_package_manager(path), None);
    }

    #[test]
    fn test_detect_pnpm_priority() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        // Create multiple lockfiles - pnpm should win
        fs::write(path.join("package.json"), "{}").unwrap();
        fs::write(path.join("pnpm-lock.yaml"), "").unwrap();
        fs::write(path.join("package-lock.json"), "").unwrap();

        assert_eq!(detect_package_manager(path), Some(PackageManager::Pnpm));
    }

    #[test]
    fn test_detect_pnpm_version_5() {
        let tmp = TempDir::new().unwrap();
        let lockfile = tmp.path().join("pnpm-lock.yaml");

        fs::write(&lockfile, "lockfileVersion: 5.4\n").unwrap();

        assert_eq!(detect_pnpm_version(&lockfile), Some("7".to_string()));
    }

    #[test]
    fn test_detect_pnpm_version_6() {
        let tmp = TempDir::new().unwrap();
        let lockfile = tmp.path().join("pnpm-lock.yaml");

        fs::write(&lockfile, "lockfileVersion: '6.0'\n").unwrap();

        assert_eq!(detect_pnpm_version(&lockfile), Some("8".to_string()));
    }

    #[test]
    fn test_detect_pnpm_version_7() {
        let tmp = TempDir::new().unwrap();
        let lockfile = tmp.path().join("pnpm-lock.yaml");

        fs::write(&lockfile, "lockfileVersion: \"7.0\"\n").unwrap();

        assert_eq!(detect_pnpm_version(&lockfile), Some("9".to_string()));
    }

    #[test]
    fn test_detect_pnpm_version_missing() {
        let tmp = TempDir::new().unwrap();
        let lockfile = tmp.path().join("pnpm-lock.yaml");

        fs::write(&lockfile, "dependencies: {}\n").unwrap();

        assert_eq!(detect_pnpm_version(&lockfile), None);
    }
}
