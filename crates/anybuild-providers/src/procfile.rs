//! Procfile parsing (port of `procfile.py`, itself forked from honcho).

use anyhow::{bail, Result};
use indexmap::IndexMap;

pub struct Procfile {
    pub processes: IndexMap<String, String>,
}

impl Procfile {
    pub fn loads(contents: &str) -> Result<Self> {
        let line_re = regex::Regex::new(r"^([A-Za-z0-9_-]+):\s*(.+)$").unwrap();
        let mut processes = IndexMap::new();
        for line in contents.lines() {
            if let Some(captures) = line_re.captures(line) {
                let name = captures[1].to_owned();
                let command = captures[2].to_owned();
                if processes.contains_key(&name) {
                    bail!("process names must be unique within a Procfile");
                }
                processes.insert(name, command);
            }
        }
        Ok(Self { processes })
    }

    pub fn get_start_command(&self) -> Option<&str> {
        for key in ["web", "default", "start"] {
            if let Some(command) = self.processes.get(key) {
                return Some(command);
            }
        }
        if self.processes.len() == 1 {
            return self.processes.values().next().map(String::as_str);
        }
        None
    }
}
