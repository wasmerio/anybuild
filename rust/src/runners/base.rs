//! Base runner trait and common utilities.

use crate::types::serve::{PrepareStep, Serve};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Runner trait for executing serve commands.
pub trait Runner {
    /// Get the serve mount path (where files are accessed during serving).
    fn get_serve_mount_path(&self, name: &str) -> PathBuf;

    /// Build the runner (generate scripts/manifests).
    fn build(&mut self, serve: &Serve) -> Result<()>;

    /// Run prepare steps before serving.
    fn prepare(&self, env: &HashMap<String, String>, prepare: &[PrepareStep]) -> Result<()>;

    /// Run a serve command.
    fn run_serve_command(&self, command: &str) -> Result<()>;
}

/// Generate a bash script from commands.
pub fn generate_bash_script(commands: &[String], cwd: Option<&Path>) -> String {
    let mut lines = vec![
        "#!/bin/bash".to_string(),
        "set -e".to_string(),
        "".to_string(),
    ];

    if let Some(dir) = cwd {
        lines.push(format!("cd {}", dir.display()));
        lines.push("".to_string());
    }

    for command in commands {
        lines.push(command.clone());
    }

    lines.join("\n")
}

/// Make a file executable (Unix-style).
pub fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)
            .with_context(|| format!("Failed to get metadata for {}", path.display()))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms)
            .with_context(|| format!("Failed to set permissions for {}", path.display()))?;
    }
    #[cfg(not(unix))]
    {
        // On Windows, just ensure the file exists
        let _ = path;
    }
    Ok(())
}

/// Format environment variables for bash.
pub fn format_env_vars(env: &HashMap<String, String>) -> Vec<String> {
    let mut lines = vec![];
    for (key, value) in env {
        lines.push(format!("export {}=\"{}\"", key, value));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_bash_script_simple() {
        let commands = vec!["echo hello".to_string(), "echo world".to_string()];
        let script = generate_bash_script(&commands, None);
        assert!(script.starts_with("#!/bin/bash"));
        assert!(script.contains("echo hello"));
        assert!(script.contains("echo world"));
    }

    #[test]
    fn test_generate_bash_script_with_cwd() {
        let commands = vec!["npm start".to_string()];
        let cwd = PathBuf::from("/app");
        let script = generate_bash_script(&commands, Some(&cwd));
        assert!(script.contains("cd /app"));
        assert!(script.contains("npm start"));
    }

    #[test]
    fn test_format_env_vars() {
        let mut env = HashMap::new();
        env.insert("PORT".to_string(), "3000".to_string());
        env.insert("HOST".to_string(), "0.0.0.0".to_string());
        let lines = format_env_vars(&env);
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().any(|l| l.contains("PORT=\"3000\"")));
    }
}
