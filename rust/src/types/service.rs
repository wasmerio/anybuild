//! Service type definition

use allocative::Allocative;
use serde::{Deserialize, Serialize};

/// Service provider types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Allocative)]
#[serde(rename_all = "lowercase")]
pub enum ServiceProvider {
    Postgres,
    Mysql,
    Redis,
}

/// A service dependency
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Allocative)]
pub struct Service {
    /// Name of the service
    pub name: String,
    /// Service provider
    pub provider: ServiceProvider,
}

impl Service {
    /// Create a new service
    pub fn new(name: impl Into<String>, provider: ServiceProvider) -> Self {
        Self {
            name: name.into(),
            provider,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_creation() {
        let service = Service::new("db", ServiceProvider::Postgres);
        assert_eq!(service.name, "db");
        assert_eq!(service.provider, ServiceProvider::Postgres);
    }

    #[test]
    fn test_service_serialization() {
        let service = Service::new("cache", ServiceProvider::Redis);
        let json = serde_json::to_string(&service).unwrap();
        let deserialized: Service = serde_json::from_str(&json).unwrap();
        assert_eq!(service, deserialized);
    }

    #[test]
    fn test_service_provider_serialization() {
        let json = serde_json::to_string(&ServiceProvider::Postgres).unwrap();
        assert_eq!(json, r#""postgres""#);

        let json = serde_json::to_string(&ServiceProvider::Mysql).unwrap();
        assert_eq!(json, r#""mysql""#);

        let json = serde_json::to_string(&ServiceProvider::Redis).unwrap();
        assert_eq!(json, r#""redis""#);
    }
}
