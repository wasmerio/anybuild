//! Custom commands configuration

use serde::{Deserialize, Serialize};

/// Custom commands that can override default behavior
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomCommands {
    /// Install command (e.g., "npm install")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install: Option<String>,
    /// Build command (e.g., "npm run build")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
    /// Start command (e.g., "npm start")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    /// Post-deployment command
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_deploy: Option<String>,
}

impl CustomCommands {
    /// Create empty custom commands
    pub fn new() -> Self {
        Self::default()
    }

    /// Set install command
    pub fn with_install(mut self, install: impl Into<String>) -> Self {
        self.install = Some(install.into());
        self
    }

    /// Set build command
    pub fn with_build(mut self, build: impl Into<String>) -> Self {
        self.build = Some(build.into());
        self
    }

    /// Set start command
    pub fn with_start(mut self, start: impl Into<String>) -> Self {
        self.start = Some(start.into());
        self
    }

    /// Set after_deploy command
    pub fn with_after_deploy(mut self, after_deploy: impl Into<String>) -> Self {
        self.after_deploy = Some(after_deploy.into());
        self
    }

    /// Check if any commands are set
    pub fn is_empty(&self) -> bool {
        self.install.is_none()
            && self.build.is_none()
            && self.start.is_none()
            && self.after_deploy.is_none()
    }

    /// Validate commands (ensure non-empty strings)
    pub fn validate(&self) -> anyhow::Result<()> {
        let check_not_empty = |cmd: &Option<String>, name: &str| {
            if let Some(s) = cmd {
                if s.trim().is_empty() {
                    anyhow::bail!("{} command cannot be empty", name);
                }
            }
            Ok(())
        };

        check_not_empty(&self.install, "Install")?;
        check_not_empty(&self.build, "Build")?;
        check_not_empty(&self.start, "Start")?;
        check_not_empty(&self.after_deploy, "After deploy")?;

        Ok(())
    }

    /// Merge with another CustomCommands, preferring values from other
    pub fn merge(mut self, other: CustomCommands) -> Self {
        if other.install.is_some() {
            self.install = other.install;
        }
        if other.build.is_some() {
            self.build = other.build;
        }
        if other.start.is_some() {
            self.start = other.start;
        }
        if other.after_deploy.is_some() {
            self.after_deploy = other.after_deploy;
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_commands_default() {
        let cmd = CustomCommands::default();
        assert!(cmd.is_empty());
        assert!(cmd.install.is_none());
    }

    #[test]
    fn test_custom_commands_builder() {
        let cmd = CustomCommands::new()
            .with_install("npm install")
            .with_build("npm run build")
            .with_start("npm start");

        assert!(!cmd.is_empty());
        assert_eq!(cmd.install, Some("npm install".to_string()));
        assert_eq!(cmd.build, Some("npm run build".to_string()));
        assert_eq!(cmd.start, Some("npm start".to_string()));
    }

    #[test]
    fn test_validation_empty_strings() {
        let cmd = CustomCommands::new().with_build("   ");
        assert!(cmd.validate().is_err());

        let cmd = CustomCommands::new().with_build("npm run build");
        assert!(cmd.validate().is_ok());
    }

    #[test]
    fn test_merge() {
        let cmd1 = CustomCommands::new()
            .with_install("npm install")
            .with_build("npm run build");

        let cmd2 = CustomCommands::new()
            .with_build("yarn build")
            .with_start("yarn start");

        let merged = cmd1.merge(cmd2);

        assert_eq!(merged.install, Some("npm install".to_string()));
        assert_eq!(merged.build, Some("yarn build".to_string()));
        assert_eq!(merged.start, Some("yarn start".to_string()));
    }

    #[test]
    fn test_serialization() {
        let cmd = CustomCommands::new()
            .with_build("npm run build")
            .with_start("npm start");

        let json = serde_json::to_string(&cmd).unwrap();
        let deserialized: CustomCommands = serde_json::from_str(&json).unwrap();

        assert_eq!(cmd, deserialized);
    }
}
