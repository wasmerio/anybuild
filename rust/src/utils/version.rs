//! Version information

/// Current version of shipit-cli
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_format() {
        // Version should be in semver format
        assert!(!VERSION.is_empty());
        assert!(VERSION.contains('.'));
    }
}
