//! Package mapper for Wasmer dependencies.

use once_cell::sync::Lazy;
use std::collections::HashMap;

/// Binary rewrite map (rewrites binaries to use python -m pattern).
pub static REWRITE_BINARIES: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("python", "python");
    m.insert("python3", "python");
    m.insert("python3.13", "python");
    m.insert("daphne", "python -m daphne");
    m.insert("gunicorn", "python -m gunicorn");
    m.insert("uvicorn", "python -m uvicorn");
    m.insert("alembic", "python -m alembic");
    m.insert("hypercorn", "python -m hypercorn");
    m.insert("fastapi", "python -m fastapi");
    m.insert("streamlit", "python -m streamlit");
    m.insert("flask", "python -m flask");
    m.insert("mcp", "python -m mcp");
    m
});

/// Mapper item for package dependencies.
#[derive(Debug, Clone)]
pub struct MapperItem {
    /// Dependencies map (version -> wasmer package reference)
    pub dependencies: HashMap<String, String>,
    /// Architecture-specific dependencies (architecture -> version map)
    pub architecture_dependencies: Option<HashMap<String, HashMap<String, String>>>,
    /// Script binaries provided
    pub scripts: Vec<String>,
    /// Binary aliases
    pub aliases: HashMap<String, String>,
    /// Environment variables to set
    pub env: HashMap<String, String>,
}

/// Package mapper for Wasmer dependencies.
pub static PACKAGE_MAPPER: Lazy<HashMap<&'static str, MapperItem>> = Lazy::new(|| {
    let mut m = HashMap::new();

    // Python
    m.insert(
        "python",
        MapperItem {
            dependencies: [("3.13".to_string(), "python/python@=3.13.5".to_string())]
                .iter()
                .cloned()
                .collect(),
            architecture_dependencies: None,
            scripts: vec!["python".to_string(), "python3".to_string()],
            aliases: [
                ("python3".to_string(), "python".to_string()),
                ("python3.13".to_string(), "python".to_string()),
            ]
            .iter()
            .cloned()
            .collect(),
            env: HashMap::new(),
        },
    );

    // PHP (architecture-dependent)
    let mut php_arch_deps = HashMap::new();
    php_arch_deps.insert(
        "32bit".to_string(),
        [
            ("8.3".to_string(), "php/php@8.3.11-linux-i386".to_string()),
            ("8.2".to_string(), "php/php@8.2.23-linux-i386".to_string()),
            ("8.1".to_string(), "php/php@8.1.29-linux-i386".to_string()),
            ("8.0".to_string(), "php/php@8.0.30-linux-i386".to_string()),
            ("7.4".to_string(), "php/php@7.4.33-linux-i386".to_string()),
        ]
        .iter()
        .cloned()
        .collect(),
    );
    php_arch_deps.insert(
        "64bit".to_string(),
        [
            ("8.3".to_string(), "php/php@8.3.11-linux-x64".to_string()),
            ("8.2".to_string(), "php/php@8.2.23-linux-x64".to_string()),
            ("8.1".to_string(), "php/php@8.1.29-linux-x64".to_string()),
            ("8.0".to_string(), "php/php@8.0.30-linux-x64".to_string()),
            ("7.4".to_string(), "php/php@7.4.33-linux-x64".to_string()),
        ]
        .iter()
        .cloned()
        .collect(),
    );

    m.insert(
        "php",
        MapperItem {
            dependencies: HashMap::new(),
            architecture_dependencies: Some(php_arch_deps),
            scripts: vec!["php".to_string()],
            aliases: HashMap::new(),
            env: HashMap::new(),
        },
    );

    // Bash
    m.insert(
        "bash",
        MapperItem {
            dependencies: [("latest".to_string(), "wasmer/bash@=1.0.24".to_string())]
                .iter()
                .cloned()
                .collect(),
            architecture_dependencies: None,
            scripts: vec!["bash".to_string()],
            aliases: HashMap::new(),
            env: HashMap::new(),
        },
    );

    // Static web server
    m.insert(
        "static-web-server",
        MapperItem {
            dependencies: [(
                "latest".to_string(),
                "wasmer/static-web-server@=1.1.0".to_string(),
            )]
            .iter()
            .cloned()
            .collect(),
            architecture_dependencies: None,
            scripts: vec!["static-web-server".to_string()],
            aliases: HashMap::new(),
            env: HashMap::new(),
        },
    );

    // Pandoc
    m.insert(
        "pandoc",
        MapperItem {
            dependencies: [("latest".to_string(), "wasmer/pandoc@=0.0.1".to_string())]
                .iter()
                .cloned()
                .collect(),
            architecture_dependencies: None,
            scripts: vec!["pandoc".to_string()],
            aliases: HashMap::new(),
            env: HashMap::new(),
        },
    );

    // FFmpeg
    m.insert(
        "ffmpeg",
        MapperItem {
            dependencies: [("latest".to_string(), "wasmer/ffmpeg@=1.0.5".to_string())]
                .iter()
                .cloned()
                .collect(),
            architecture_dependencies: None,
            scripts: vec!["ffmpeg".to_string(), "ffprobe".to_string()],
            aliases: HashMap::new(),
            env: HashMap::new(),
        },
    );

    m
});

/// Get mapper item for a package.
pub fn get_mapper_item(name: &str) -> Option<&MapperItem> {
    PACKAGE_MAPPER.get(name)
}

/// Get dependency version string.
pub fn get_dependency_version(
    name: &str,
    version: Option<&str>,
    arch: Option<&str>,
) -> anyhow::Result<String> {
    let mapper = get_mapper_item(name)
        .ok_or_else(|| anyhow::anyhow!("Package '{}' not found in mapper", name))?;

    let version = version.unwrap_or("latest");

    // Check architecture-specific dependencies first
    if let Some(arch_deps) = &mapper.architecture_dependencies {
        if let Some(arch_str) = arch {
            if let Some(arch_map) = arch_deps.get(arch_str) {
                if let Some(pkg) = arch_map.get(version) {
                    return Ok(pkg.clone());
                }
            }
        }
    }

    // Fall back to regular dependencies
    mapper
        .dependencies
        .get(version)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Version '{}' not found for package '{}'", version, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rewrite_binaries() {
        assert_eq!(REWRITE_BINARIES.get("python"), Some(&"python"));
        assert_eq!(REWRITE_BINARIES.get("uvicorn"), Some(&"python -m uvicorn"));
        assert_eq!(REWRITE_BINARIES.get("unknown"), None);
    }

    #[test]
    fn test_get_mapper_item_python() {
        let item = get_mapper_item("python").unwrap();
        assert!(item.scripts.contains(&"python".to_string()));
        assert!(!item.dependencies.is_empty());
    }

    #[test]
    fn test_get_mapper_item_unknown() {
        assert!(get_mapper_item("unknown").is_none());
    }

    #[test]
    fn test_get_dependency_version_python() {
        let result = get_dependency_version("python", Some("3.13"), None).unwrap();
        assert!(result.contains("python/python"));
    }

    #[test]
    fn test_get_dependency_version_php_64bit() {
        let result = get_dependency_version("php", Some("8.3"), Some("64bit")).unwrap();
        assert!(result.contains("php/php@8.3"));
        assert!(result.contains("x64"));
    }

    #[test]
    fn test_get_dependency_version_php_32bit() {
        let result = get_dependency_version("php", Some("8.3"), Some("32bit")).unwrap();
        assert!(result.contains("php/php@8.3"));
        assert!(result.contains("i386"));
    }

    #[test]
    fn test_get_dependency_version_unknown_package() {
        let result = get_dependency_version("unknown", None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_dependency_version_unknown_version() {
        let result = get_dependency_version("python", Some("999"), None);
        assert!(result.is_err());
    }
}
