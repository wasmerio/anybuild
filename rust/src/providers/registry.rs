//! Provider registry system

use crate::providers::go::GoProvider;
use crate::providers::hugo::HugoProvider;
use crate::providers::laravel::LaravelProvider;
use crate::providers::mkdocs::MkDocsProvider;
use crate::providers::node::NodeStaticProvider;
use crate::providers::php::PhpProvider;
use crate::providers::python::PythonProvider;
use crate::providers::staticfile::StaticfileProvider;
use crate::providers::{DetectResult, Provider};
use crate::starlark::config::ShipitConfig;
use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

/// Registry for managing providers
pub struct ProviderRegistry {
    providers: Vec<Arc<dyn Provider>>,
}

impl ProviderRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Create a registry with default providers
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();

        // Register providers in priority order (higher priority = checked first)
        registry.register(Arc::new(LaravelProvider)); // 10
        registry.register(Arc::new(NodeStaticProvider)); // 8
        registry.register(Arc::new(HugoProvider)); // 8
        registry.register(Arc::new(MkDocsProvider)); // 8
        registry.register(Arc::new(PythonProvider)); // 6
        registry.register(Arc::new(GoProvider)); // 5
        registry.register(Arc::new(PhpProvider)); // 5
        registry.register(Arc::new(StaticfileProvider)); // 1 (fallback)

        registry
    }

    /// Register a provider
    pub fn register(&mut self, provider: Arc<dyn Provider>) {
        self.providers.push(provider);
        // Sort by priority (highest first)
        self.providers
            .sort_by_key(|b| std::cmp::Reverse(b.priority()));
    }

    /// Get all registered providers
    pub fn get_all(&self) -> &[Arc<dyn Provider>] {
        &self.providers
    }

    /// Detect the best matching provider for a project
    ///
    /// Returns the provider with the highest confidence score.
    pub fn detect_best(&self, path: &Path) -> Result<Option<(Arc<dyn Provider>, DetectResult)>> {
        let mut best: Option<(Arc<dyn Provider>, DetectResult)> = None;

        for provider in &self.providers {
            if let Some(result) = provider.detect(path)? {
                match &best {
                    None => best = Some((Arc::clone(provider), result)),
                    Some((_, best_result)) => {
                        if result.confidence > best_result.confidence {
                            best = Some((Arc::clone(provider), result));
                        }
                    }
                }
            }
        }

        Ok(best)
    }

    /// Detect all matching providers
    ///
    /// Returns all providers that match, sorted by confidence (highest first).
    pub fn detect_all(&self, path: &Path) -> Result<Vec<(Arc<dyn Provider>, DetectResult)>> {
        let mut results = Vec::new();

        for provider in &self.providers {
            if let Some(result) = provider.detect(path)? {
                results.push((Arc::clone(provider), result));
            }
        }

        // Sort by confidence (highest first)
        results.sort_by(|a, b| {
            b.1.confidence
                .partial_cmp(&a.1.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(results)
    }

    /// Find a provider by name
    pub fn find_by_name(&self, name: &str) -> Option<&Arc<dyn Provider>> {
        self.providers.iter().find(|p| p.name() == name)
    }

    /// Filter providers by minimum confidence
    pub fn filter_by_confidence(
        &self,
        path: &Path,
        min_confidence: f32,
    ) -> Result<Vec<(Arc<dyn Provider>, DetectResult)>> {
        let all = self.detect_all(path)?;
        Ok(all
            .into_iter()
            .filter(|(_, result)| result.confidence >= min_confidence)
            .collect())
    }

    /// Detect the best provider for a project and return its config.
    ///
    /// Returns an empty `ShipitConfig` if no provider matches.
    pub fn detect_config(
        &self,
        project_path: &Path,
    ) -> Result<ShipitConfig> {
        match self.detect_best(project_path)? {
            Some((provider, _)) => provider.provider_config(project_path),
            None => Ok(ShipitConfig::new()),
        }
    }

    /// Get the number of registered providers
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a registry with all built-in providers
pub fn create_default_registry() -> ProviderRegistry {
    let mut registry = ProviderRegistry::new();

    // Register built-in providers
    // (More will be added as they are implemented)
    registry.register(Arc::new(crate::providers::staticfile::StaticfileProvider));

    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // Mock provider for testing
    struct MockProvider {
        name: String,
        confidence: f32,
        priority: i32,
    }

    impl Provider for MockProvider {
        fn name(&self) -> &str {
            &self.name
        }

        fn detect(&self, _path: &Path) -> Result<Option<DetectResult>> {
            if self.confidence > 0.0 {
                Ok(Some(DetectResult::new(&self.name, self.confidence, "test")))
            } else {
                Ok(None)
            }
        }

        fn plan(&self, _path: &Path) -> Result<crate::providers::ProviderPlan> {
            unimplemented!()
        }

        fn priority(&self) -> i32 {
            self.priority
        }
    }

    #[test]
    fn test_registry_creation() {
        let registry = ProviderRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_register_provider() {
        let mut registry = ProviderRegistry::new();
        let provider = Arc::new(MockProvider {
            name: "test".to_string(),
            confidence: 0.8,
            priority: 0,
        });

        registry.register(provider);
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_detect_best() {
        let mut registry = ProviderRegistry::new();
        let temp = TempDir::new().unwrap();

        registry.register(Arc::new(MockProvider {
            name: "low".to_string(),
            confidence: 0.3,
            priority: 0,
        }));

        registry.register(Arc::new(MockProvider {
            name: "high".to_string(),
            confidence: 0.9,
            priority: 0,
        }));

        registry.register(Arc::new(MockProvider {
            name: "medium".to_string(),
            confidence: 0.6,
            priority: 0,
        }));

        let result = registry.detect_best(temp.path()).unwrap();
        assert!(result.is_some());

        let (provider, detect_result) = result.unwrap();
        assert_eq!(provider.name(), "high");
        assert_eq!(detect_result.confidence, 0.9);
    }

    #[test]
    fn test_detect_all() {
        let mut registry = ProviderRegistry::new();
        let temp = TempDir::new().unwrap();

        registry.register(Arc::new(MockProvider {
            name: "provider1".to_string(),
            confidence: 0.5,
            priority: 0,
        }));

        registry.register(Arc::new(MockProvider {
            name: "provider2".to_string(),
            confidence: 0.8,
            priority: 0,
        }));

        registry.register(Arc::new(MockProvider {
            name: "no-match".to_string(),
            confidence: 0.0,
            priority: 0,
        }));

        let results = registry.detect_all(temp.path()).unwrap();
        assert_eq!(results.len(), 2);

        // Should be sorted by confidence
        assert_eq!(results[0].1.confidence, 0.8);
        assert_eq!(results[1].1.confidence, 0.5);
    }

    #[test]
    fn test_find_by_name() {
        let mut registry = ProviderRegistry::new();

        registry.register(Arc::new(MockProvider {
            name: "test-provider".to_string(),
            confidence: 0.5,
            priority: 0,
        }));

        assert!(registry.find_by_name("test-provider").is_some());
        assert!(registry.find_by_name("nonexistent").is_none());
    }

    #[test]
    fn test_priority_sorting() {
        let mut registry = ProviderRegistry::new();

        registry.register(Arc::new(MockProvider {
            name: "low-priority".to_string(),
            confidence: 0.5,
            priority: -10,
        }));

        registry.register(Arc::new(MockProvider {
            name: "high-priority".to_string(),
            confidence: 0.5,
            priority: 10,
        }));

        registry.register(Arc::new(MockProvider {
            name: "default-priority".to_string(),
            confidence: 0.5,
            priority: 0,
        }));

        let providers = registry.get_all();
        assert_eq!(providers[0].name(), "high-priority");
        assert_eq!(providers[1].name(), "default-priority");
        assert_eq!(providers[2].name(), "low-priority");
    }

    #[test]
    fn test_filter_by_confidence() {
        let mut registry = ProviderRegistry::new();
        let temp = TempDir::new().unwrap();

        registry.register(Arc::new(MockProvider {
            name: "low".to_string(),
            confidence: 0.3,
            priority: 0,
        }));

        registry.register(Arc::new(MockProvider {
            name: "high".to_string(),
            confidence: 0.9,
            priority: 0,
        }));

        let results = registry.filter_by_confidence(temp.path(), 0.5).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0.name(), "high");
    }
}
