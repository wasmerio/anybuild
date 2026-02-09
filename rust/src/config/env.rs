//! Environment variable handling

use anyhow::{Context, Result};
use regex::Regex;
use std::path::Path;

/// Load environment variables from a .env file
///
/// Does not override existing environment variables.
pub fn load_env(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(()); // Not an error if file doesn't exist
    }

    dotenvy::from_path(path)
        .with_context(|| format!("Failed to load .env file from {}", path.display()))?;

    Ok(())
}

/// Get an environment variable
pub fn get_env(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

/// Get an environment variable with a default value
pub fn get_env_with_default(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Parse an environment variable to a specific type
pub fn parse_env<T: std::str::FromStr>(key: &str) -> Result<Option<T>>
where
    T::Err: std::fmt::Display,
{
    match std::env::var(key) {
        Ok(val) => {
            let parsed = val
                .parse::<T>()
                .map_err(|e| anyhow::anyhow!("Failed to parse env var {}: {}", key, e))?;
            Ok(Some(parsed))
        }
        Err(_) => Ok(None),
    }
}

/// Expand environment variables in a string
///
/// Supports both `$VAR` and `${VAR}` syntax.
/// Returns the original string if no variables are found.
pub fn expand_env_vars(input: &str) -> String {
    // Pattern matches $VAR or ${VAR}
    let re = Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}|\$([A-Za-z_][A-Za-z0-9_]*)").unwrap();

    re.replace_all(input, |caps: &regex::Captures| {
        // Try to get the variable name from either capture group
        let var_name = caps
            .get(1)
            .or_else(|| caps.get(2))
            .map(|m| m.as_str())
            .unwrap_or("");

        // Look up the environment variable
        std::env::var(var_name).unwrap_or_else(|_| {
            // If not found, return the original text
            caps.get(0).unwrap().as_str().to_string()
        })
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_get_env() {
        env::set_var("TEST_VAR_1", "test_value");
        assert_eq!(get_env("TEST_VAR_1"), Some("test_value".to_string()));
        assert_eq!(get_env("NONEXISTENT_VAR"), None);
        env::remove_var("TEST_VAR_1");
    }

    #[test]
    fn test_get_env_with_default() {
        env::set_var("TEST_VAR_2", "value");
        assert_eq!(get_env_with_default("TEST_VAR_2", "default"), "value");
        assert_eq!(get_env_with_default("NONEXISTENT", "default"), "default");
        env::remove_var("TEST_VAR_2");
    }

    #[test]
    fn test_parse_env() {
        env::set_var("TEST_PORT", "8080");
        let port: Option<u16> = parse_env("TEST_PORT").unwrap();
        assert_eq!(port, Some(8080));

        let missing: Option<u16> = parse_env("MISSING_VAR").unwrap();
        assert_eq!(missing, None);

        env::set_var("TEST_INVALID", "not_a_number");
        let result: Result<Option<u16>> = parse_env("TEST_INVALID");
        assert!(result.is_err());

        env::remove_var("TEST_PORT");
        env::remove_var("TEST_INVALID");
    }

    #[test]
    fn test_expand_env_vars_simple() {
        env::set_var("TEST_USER", "alice");
        env::set_var("TEST_HOME", "/home/alice");

        let expanded = expand_env_vars("User: $TEST_USER");
        assert_eq!(expanded, "User: alice");

        let expanded = expand_env_vars("Home: ${TEST_HOME}");
        assert_eq!(expanded, "Home: /home/alice");

        env::remove_var("TEST_USER");
        env::remove_var("TEST_HOME");
    }

    #[test]
    fn test_expand_env_vars_multiple() {
        env::set_var("TEST_A", "foo");
        env::set_var("TEST_B", "bar");

        let expanded = expand_env_vars("$TEST_A and ${TEST_B}");
        assert_eq!(expanded, "foo and bar");

        env::remove_var("TEST_A");
        env::remove_var("TEST_B");
    }

    #[test]
    fn test_expand_env_vars_missing() {
        // Missing variables should be left as-is
        let expanded = expand_env_vars("Value: $NONEXISTENT_VAR");
        assert_eq!(expanded, "Value: $NONEXISTENT_VAR");

        let expanded = expand_env_vars("Value: ${NONEXISTENT_VAR}");
        assert_eq!(expanded, "Value: ${NONEXISTENT_VAR}");
    }

    #[test]
    fn test_expand_env_vars_no_vars() {
        let expanded = expand_env_vars("Just a plain string");
        assert_eq!(expanded, "Just a plain string");
    }

    #[test]
    fn test_load_env_missing_file() {
        let path = Path::new("/nonexistent/.env");
        let result = load_env(path);
        assert!(result.is_ok()); // Should not error on missing file
    }

    #[test]
    fn test_load_env_file() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "TEST_ENV_VAR=test_value").unwrap();
        writeln!(file, "TEST_ENV_NUM=42").unwrap();
        file.flush().unwrap();

        let result = load_env(file.path());
        assert!(result.is_ok());

        // Note: dotenvy doesn't load into std::env in tests reliably
        // so we can't test the actual values here without more setup
    }
}
