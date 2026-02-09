//! Procfile parser
//!
//! Parses Heroku-style Procfile format.

use anyhow::{Context, Result};
use regex::Regex;
use std::collections::HashMap;

/// A parsed Procfile
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Procfile {
    /// Process name -> command mapping
    pub processes: HashMap<String, String>,
}

impl Procfile {
    /// Create an empty Procfile
    pub fn new() -> Self {
        Self {
            processes: HashMap::new(),
        }
    }

    /// Parse a Procfile from string contents
    pub fn loads(contents: &str) -> Result<Self> {
        let line_pattern = Regex::new(r"^([A-Za-z0-9_-]+):\s*(.+)$")
            .context("Failed to compile Procfile regex")?;

        let mut procfile = Self::new();

        for line in contents.lines() {
            if let Some(captures) = line_pattern.captures(line) {
                let name = captures.get(1).unwrap().as_str();
                let command = captures.get(2).unwrap().as_str();

                if procfile.processes.contains_key(name) {
                    anyhow::bail!(
                        "Process names must be unique within a Procfile: duplicate '{}'",
                        name
                    );
                }

                procfile.add_process(name, command);
            }
        }

        Ok(procfile)
    }

    /// Add a process to the Procfile
    pub fn add_process(&mut self, name: impl Into<String>, command: impl Into<String>) {
        self.processes.insert(name.into(), command.into());
    }

    /// Get the start command with priority: web > default > start > single process
    pub fn get_start_command(&self) -> Option<&str> {
        // Priority order
        if let Some(cmd) = self.processes.get("web") {
            return Some(cmd);
        }
        if let Some(cmd) = self.processes.get("default") {
            return Some(cmd);
        }
        if let Some(cmd) = self.processes.get("start") {
            return Some(cmd);
        }
        // If there's only one process, return it
        if self.processes.len() == 1 {
            return self.processes.values().next().map(|s| s.as_str());
        }
        None
    }
}

impl Default for Procfile {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_procfile() {
        let contents = "web: npm start\nworker: node worker.js";
        let procfile = Procfile::loads(contents).unwrap();

        assert_eq!(procfile.processes.len(), 2);
        assert_eq!(
            procfile.processes.get("web"),
            Some(&"npm start".to_string())
        );
        assert_eq!(
            procfile.processes.get("worker"),
            Some(&"node worker.js".to_string())
        );
    }

    #[test]
    fn test_get_start_command_web() {
        let contents = "web: npm start\nworker: node worker.js";
        let procfile = Procfile::loads(contents).unwrap();
        assert_eq!(procfile.get_start_command(), Some("npm start"));
    }

    #[test]
    fn test_get_start_command_default() {
        let contents = "default: python app.py\nworker: python worker.py";
        let procfile = Procfile::loads(contents).unwrap();
        assert_eq!(procfile.get_start_command(), Some("python app.py"));
    }

    #[test]
    fn test_get_start_command_start() {
        let contents = "start: ./server\nworker: ./worker";
        let procfile = Procfile::loads(contents).unwrap();
        assert_eq!(procfile.get_start_command(), Some("./server"));
    }

    #[test]
    fn test_get_start_command_single() {
        let contents = "server: python app.py";
        let procfile = Procfile::loads(contents).unwrap();
        assert_eq!(procfile.get_start_command(), Some("python app.py"));
    }

    #[test]
    fn test_get_start_command_none() {
        let contents = "worker1: ./worker1\nworker2: ./worker2";
        let procfile = Procfile::loads(contents).unwrap();
        assert_eq!(procfile.get_start_command(), None);
    }

    #[test]
    fn test_duplicate_process_name() {
        let contents = "web: npm start\nweb: node server.js";
        let result = Procfile::loads(contents);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("duplicate"));
    }

    #[test]
    fn test_empty_procfile() {
        let procfile = Procfile::loads("").unwrap();
        assert_eq!(procfile.processes.len(), 0);
        assert_eq!(procfile.get_start_command(), None);
    }

    #[test]
    fn test_comments_ignored() {
        let contents = "web: npm start\n# comment line\nworker: node worker.js";
        let procfile = Procfile::loads(contents).unwrap();
        assert_eq!(procfile.processes.len(), 2);
    }

    #[test]
    fn test_priority_order() {
        // Test that 'web' has highest priority
        let contents = "start: ./start\ndefault: ./default\nweb: ./web";
        let procfile = Procfile::loads(contents).unwrap();
        assert_eq!(procfile.get_start_command(), Some("./web"));

        // Test that 'default' has priority over 'start'
        let contents = "start: ./start\ndefault: ./default\nworker: ./worker";
        let procfile = Procfile::loads(contents).unwrap();
        assert_eq!(procfile.get_start_command(), Some("./default"));
    }
}
