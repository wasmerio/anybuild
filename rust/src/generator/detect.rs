//! Provider detection for automatic Shipit file generation.

use crate::config::Config;
use crate::providers::{Provider, ProviderRegistry};
use anyhow::{anyhow, Result};
use std::path::Path;
use std::sync::Arc;

/// Detect the best matching provider for a project.
pub fn detect_provider(
    path: &Path,
    registry: &ProviderRegistry,
    _config: &Config,
) -> Result<Arc<dyn Provider>> {
    let result = registry
        .detect_best(path)?
        .ok_or_else(|| anyhow!("No suitable provider found for project at {:?}", path))?;

    Ok(result.0)
}

/// Load a provider either by name or through auto-detection.
pub fn load_provider(
    path: &Path,
    registry: &ProviderRegistry,
    config: &Config,
    provider_name: Option<&str>,
) -> Result<Arc<dyn Provider>> {
    if let Some(name) = provider_name {
        // Find provider by name
        registry
            .get_all()
            .iter()
            .find(|p| p.name() == name)
            .map(Arc::clone)
            .ok_or_else(|| anyhow!("Provider '{}' not found", name))
    } else {
        // Auto-detect
        detect_provider(path, registry, config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_detect_provider_no_match() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path();

        let registry = ProviderRegistry::with_defaults();
        let config = Config::default();

        // Empty directory should match staticfile provider (fallback)
        let result = detect_provider(path, &registry, &config);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name(), "staticfile");
    }

    #[test]
    fn test_load_provider_by_name() {
        let tmp = TempDir::new().unwrap();
        let registry = ProviderRegistry::with_defaults();
        let config = Config::default();

        let result = load_provider(tmp.path(), &registry, &config, Some("python"));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name(), "python");
    }

    #[test]
    fn test_load_provider_invalid_name() {
        let tmp = TempDir::new().unwrap();
        let registry = ProviderRegistry::with_defaults();
        let config = Config::default();

        let result = load_provider(tmp.path(), &registry, &config, Some("nonexistent"));
        assert!(result.is_err());
    }
}
