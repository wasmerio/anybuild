//! Minimal Procfile parser used for deriving custom commands.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, anyhow};
use regex::Regex;

/// Parsed Procfile with process name to command mapping.
#[derive(Debug, Clone, Default)]
pub struct Procfile {
    pub processes: BTreeMap<String, String>,
}

impl Procfile {
    /// Parse Procfile contents from a string.
    pub fn parse(contents: &str) -> Result<Self, anyhow::Error> {
        let mut procfile = Procfile::default();
        // Match `name: command`, allowing trailing whitespace.
        let re = Regex::new(r"^([A-Za-z0-9_-]+):\s*(.+)$").unwrap();
        for line in contents.lines() {
            if let Some(caps) = re.captures(line) {
                let name = caps.get(1).unwrap().as_str().to_string();
                let command = caps.get(2).unwrap().as_str().to_string();
                if procfile.processes.contains_key(&name) {
                    return Err(anyhow!("process names must be unique: {name}"));
                }
                procfile.processes.insert(name, command);
            }
        }
        Ok(procfile)
    }

    /// Load a Procfile from disk.
    pub fn load(path: &Path) -> Result<Self, anyhow::Error> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("reading Procfile at {}", path.display()))?;
        Self::parse(&contents)
    }

    /// Return the primary start command following Procfile conventions.
    pub fn start_command(&self) -> Option<String> {
        if let Some(cmd) = self.processes.get("web") {
            return Some(cmd.clone());
        }
        if let Some(cmd) = self.processes.get("default") {
            return Some(cmd.clone());
        }
        if let Some(cmd) = self.processes.get("start") {
            return Some(cmd.clone());
        }
        if self.processes.len() == 1 {
            return self.processes.values().next().cloned();
        }
        None
    }
}
