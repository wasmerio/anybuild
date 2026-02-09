//! Serve configuration type

use crate::types::{Mount, Package, Service, Volume};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Step type for prepare phase (currently only RunStep)
pub type PrepareStep = crate::types::RunStep;

/// Serve configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Serve {
    /// Name of the serve configuration
    pub name: String,
    /// Provider name
    pub provider: String,
    /// Build steps
    pub build: Vec<crate::types::Step>,
    /// Package dependencies
    pub deps: Vec<Package>,
    /// Commands to run (e.g., web, worker)
    pub commands: HashMap<String, String>,
    /// Working directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Prepare steps (run before serve)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prepare: Option<Vec<PrepareStep>>,
    /// Worker commands
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workers: Option<Vec<String>>,
    /// Mounts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mounts: Option<Vec<Mount>>,
    /// Volumes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volumes: Option<Vec<Volume>>,
    /// Environment variables
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    /// Services
    #[serde(skip_serializing_if = "Option::is_none")]
    pub services: Option<Vec<Service>>,
}

impl Serve {
    /// Create a new serve configuration
    pub fn new(
        name: impl Into<String>,
        provider: impl Into<String>,
        build: Vec<crate::types::Step>,
        deps: Vec<Package>,
        commands: HashMap<String, String>,
    ) -> Self {
        Self {
            name: name.into(),
            provider: provider.into(),
            build,
            deps,
            commands,
            cwd: None,
            prepare: None,
            workers: None,
            mounts: None,
            volumes: None,
            env: None,
            services: None,
        }
    }

    /// Set working directory
    pub fn with_cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Set prepare steps
    pub fn with_prepare(mut self, prepare: Vec<PrepareStep>) -> Self {
        self.prepare = Some(prepare);
        self
    }

    /// Set workers
    pub fn with_workers(mut self, workers: Vec<String>) -> Self {
        self.workers = Some(workers);
        self
    }

    /// Set mounts
    pub fn with_mounts(mut self, mounts: Vec<Mount>) -> Self {
        self.mounts = Some(mounts);
        self
    }

    /// Set volumes
    pub fn with_volumes(mut self, volumes: Vec<Volume>) -> Self {
        self.volumes = Some(volumes);
        self
    }

    /// Set environment variables
    pub fn with_env(mut self, env: HashMap<String, String>) -> Self {
        self.env = Some(env);
        self
    }

    /// Set services
    pub fn with_services(mut self, services: Vec<Service>) -> Self {
        self.services = Some(services);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RunStep;

    #[test]
    fn test_serve_creation() {
        let serve = Serve::new(
            "my-app",
            "node",
            vec![],
            vec![],
            HashMap::from([("web".to_string(), "npm start".to_string())]),
        );
        assert_eq!(serve.name, "my-app");
        assert_eq!(serve.provider, "node");
        assert_eq!(serve.commands.get("web").unwrap(), "npm start");
    }

    #[test]
    fn test_serve_builder() {
        let serve = Serve::new("app", "python", vec![], vec![], HashMap::new())
            .with_cwd("/app")
            .with_prepare(vec![RunStep {
                command: "python migrate.py".to_string(),
                inputs: None,
                outputs: None,
                group: None,
            }]);

        assert_eq!(serve.cwd, Some("/app".to_string()));
        assert!(serve.prepare.is_some());
        assert_eq!(serve.prepare.as_ref().unwrap().len(), 1);
    }
}
