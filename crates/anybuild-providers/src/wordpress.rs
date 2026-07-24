//! Provider implementation.
//!
//! WordPress site detection plus plugin/theme extension detection via the
//! standard file-header comments (Plugin Name / Theme Name / Text Domain).

use std::path::Path;
use std::sync::OnceLock;

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::base::{env_enum, env_str, is_blank, BaseConfig, DetectResult, HasBase};
use crate::php::{self, PhpConfig};

pub const NAME: &str = "wordpress";

/// Python reads this many characters from the top of a candidate file.
const HEADER_SCAN_BYTES: usize = 8192;

fn plugin_header_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?im)^[ \t/*#@]*Plugin Name\s*:").expect("valid regex"))
}

fn theme_header_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?im)^[ \t/*#@]*Theme Name\s*:").expect("valid regex"))
}

fn text_domain_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?im)^[ \t/*#@]*Text Domain\s*:\s*([^\s*/#@]+)").expect("valid regex")
    })
}

fn slugify_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[^a-z0-9_-]+").expect("valid regex"))
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WordPressConfig {
    #[serde(flatten)]
    pub php: PhpConfig,
    pub wp_version: Option<String>,
    pub wp_locale: Option<String>,
    pub wp_cli_version: Option<String>,
    /// Set when the project is a plugin/theme rather than a full site.
    pub wp_extension_kind: Option<String>,
    pub wp_extension_slug: Option<String>,
    pub wp_extension_activate_target: Option<String>,
}

impl HasBase for WordPressConfig {
    fn base(&self) -> &BaseConfig {
        self.php.base()
    }
    fn base_mut(&mut self) -> &mut BaseConfig {
        self.php.base_mut()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WordPressExtension {
    /// `"plugin"` or `"theme"`.
    pub kind: &'static str,
    pub slug: String,
    pub activate_target: String,
    pub source_file: Option<String>,
}

/// Port of `WordPressProvider.load_config`.
pub fn load_config(path: &Path, base: BaseConfig) -> Result<WordPressConfig> {
    let php_config = php::load_config(path, base)?;
    // Python re-instantiates as WordPressConfig(**php_config.model_dump()):
    // every php field is an init kwarg, so the env only reaches the new
    // wp_* fields.
    let mut config = WordPressConfig {
        php: php_config,
        wp_version: env_str("wp_version"),
        wp_locale: env_str("wp_locale"),
        wp_cli_version: env_str("wp_cli_version"),
        wp_extension_kind: env_enum("wp_extension_kind", "plugin or theme", |value| {
            matches!(value, "plugin" | "theme").then(|| value.to_owned())
        })?,
        wp_extension_slug: env_str("wp_extension_slug"),
        wp_extension_activate_target: env_str("wp_extension_activate_target"),
    };
    let extension = detect_extension(path);
    // Python: `if extension and not wp_config.wp_version:` — "" is falsy.
    if extension.is_some() && is_blank(&config.wp_version) {
        config.wp_version = Some("latest".to_owned());
    }
    if let Some(extension) = extension {
        config.wp_extension_kind = Some(extension.kind.to_owned());
        config.wp_extension_slug = Some(extension.slug);
        config.wp_extension_activate_target = Some(extension.activate_target);
    }
    Ok(config)
}

/// Port of `WordPressProvider.detect`.
pub fn detect(path: &Path, _base: &BaseConfig) -> Option<DetectResult> {
    if path.join("wp-content").exists()
        && path.join("index.php").exists()
        && path.join("wp-load.php").exists()
    {
        return Some(DetectResult {
            name: NAME,
            score: 80,
        });
    }

    // Python truthiness: an empty ANYBUILD_WP_VERSION does not force detection.
    if env_str("wp_version").is_some_and(|v| !v.is_empty()) {
        return Some(DetectResult {
            name: NAME,
            score: 80,
        });
    }
    if detect_extension(path).is_some() {
        return Some(DetectResult {
            name: NAME,
            score: 75,
        });
    }
    None
}

/// Port of `WordPressProvider.detect_extension`.
pub fn detect_extension(path: &Path) -> Option<WordPressExtension> {
    detect_theme(path).or_else(|| detect_plugin(path))
}

fn detect_plugin(path: &Path) -> Option<WordPressExtension> {
    let mut plugin_files: Vec<_> = std::fs::read_dir(path)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "php"))
        .collect();
    plugin_files.sort();
    for plugin_file in plugin_files {
        if !file_has_header(&plugin_file, plugin_header_re()) {
            continue;
        }
        let slug =
            detect_text_domain_slug(&plugin_file).unwrap_or_else(|| slugify(&dir_name(path)));
        let file_name = plugin_file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        return Some(WordPressExtension {
            kind: "plugin",
            activate_target: format!("{slug}/{file_name}"),
            slug,
            source_file: Some(file_name),
        });
    }
    None
}

fn detect_theme(path: &Path) -> Option<WordPressExtension> {
    let style_css = path.join("style.css");
    if !file_has_header(&style_css, theme_header_re()) {
        return None;
    }
    if !(path.join("index.php").is_file()
        || path.join("templates/index.html").is_file()
        || path.join("theme.json").is_file())
    {
        return None;
    }
    let slug = detect_text_domain_slug(&style_css).unwrap_or_else(|| slugify(&dir_name(path)));
    Some(WordPressExtension {
        kind: "theme",
        activate_target: slug.clone(),
        slug,
        source_file: Some("style.css".to_owned()),
    })
}

fn dir_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn detect_text_domain_slug(header_file: &Path) -> Option<String> {
    let text_domain = file_header_value(header_file, text_domain_re())?;
    Some(slugify(&text_domain))
}

fn file_has_header(path: &Path, pattern: &Regex) -> bool {
    file_header_value(path, pattern).is_some()
}

fn file_header_value(path: &Path, pattern: &Regex) -> Option<String> {
    if !path.is_file() {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let contents: String = String::from_utf8_lossy(&bytes)
        .chars()
        .take(HEADER_SCAN_BYTES)
        .collect();
    let captures = pattern.captures(&contents)?;
    match captures.get(1) {
        Some(group) => Some(group.as_str().trim().to_owned()),
        None => Some(captures.get(0)?.as_str().trim().to_owned()),
    }
}

/// Port of `WordPressProvider._slugify`.
fn slugify(value: &str) -> String {
    let lowered = value.to_lowercase();
    let slug = slugify_re().replace_all(&lowered, "-");
    let slug = slug.trim_matches(['-', '_']);
    if slug.is_empty() {
        "wordpress-extension".to_owned()
    } else {
        slug.to_owned()
    }
}

#[cfg(test)]
mod tests {
    //! Port of `tests/test_wordpress_phpix.py` (the config-level halves).
    //!
    //! The plan-level assertions of the Python suite (the `--version=latest`
    //! install step, the `wpcontent_base/{plugins,themes}/<slug>` copy with
    //! `[".git", ".source"]` ignores, the WP_PLUGINS_ACTIVATE /
    //! WP_DEFAULT_THEME env, and the phpix `--startup-script=…/start-wp.php`
    //! start command with the `wordpress/start.php` asset copy and the phpix
    //! dep) are pinned byte-for-byte by the php-wordpress-plugin,
    //! php-wordpress-theme, and php-wordpress-empty plan snapshots
    //! (`crates/anybuild-starlark/tests/snapshots.rs`); the config fields that
    //! drive them are asserted here.

    use std::path::{Path, PathBuf};

    use crate::{
        load_provider, load_provider_config, merge_config_json, BaseConfig, ProviderConfig,
    };

    fn write_plugin(project_dir: &Path, filename: &str) {
        std::fs::create_dir_all(project_dir).unwrap();
        std::fs::write(
            project_dir.join(filename),
            "<?php\n/**\n * Plugin Name: My Plugin\n */\n",
        )
        .unwrap();
    }

    fn write_theme(project_dir: &Path) {
        std::fs::create_dir_all(project_dir).unwrap();
        std::fs::write(
            project_dir.join("style.css"),
            "/*\nTheme Name: My Theme\n*/\n",
        )
        .unwrap();
        std::fs::write(project_dir.join("index.php"), "<?php\n").unwrap();
    }

    fn load_for_path(project_dir: &Path) -> (&'static str, ProviderConfig) {
        let mut base = BaseConfig::default();
        base.commands.enrich_from_path(project_dir);
        let provider = load_provider(project_dir, &base, None).unwrap();
        let config = load_provider_config(provider, project_dir, base).unwrap();
        (provider, config)
    }

    fn wordpress(config: &ProviderConfig) -> &super::WordPressConfig {
        match config {
            ProviderConfig::Wordpress(config) => config,
            other => panic!("expected a wordpress config, got {other:?}"),
        }
    }

    #[test]
    fn test_wordpress_provider_detects_plugin_and_generates_activation() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("my-plugin");
        write_plugin(&project_dir, "my-plugin.php");

        let (provider, config) = load_for_path(&project_dir);
        assert_eq!(provider, "wordpress");
        let config = wordpress(&config);
        assert_eq!(config.wp_version.as_deref(), Some("latest"));
        // These feed the plugin copy target and WP_PLUGINS_ACTIVATE.
        assert_eq!(config.wp_extension_kind.as_deref(), Some("plugin"));
        assert_eq!(config.wp_extension_slug.as_deref(), Some("my-plugin"));
        assert_eq!(
            config.wp_extension_activate_target.as_deref(),
            Some("my-plugin/my-plugin.php")
        );
    }

    #[test]
    fn test_wordpress_provider_detects_theme_and_generates_activation() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("my-theme");
        write_theme(&project_dir);

        let (provider, config) = load_for_path(&project_dir);
        assert_eq!(provider, "wordpress");
        let config = wordpress(&config);
        assert_eq!(config.wp_version.as_deref(), Some("latest"));
        // These feed the theme copy target and WP_DEFAULT_THEME.
        assert_eq!(config.wp_extension_kind.as_deref(), Some("theme"));
        assert_eq!(config.wp_extension_slug.as_deref(), Some("my-theme"));
        assert_eq!(
            config.wp_extension_activate_target.as_deref(),
            Some("my-theme")
        );
    }

    #[test]
    fn test_wordpress_extension_keeps_user_wp_version() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("my-plugin");
        write_plugin(&project_dir, "my-plugin.php");

        // `load_provider_config(..., {"wp_version": "6.8.3"})` in Python:
        // load, then merge the user config over the model dump.
        let config =
            load_provider_config("wordpress", &project_dir, BaseConfig::default()).unwrap();
        let config = merge_config_json(
            "wordpress",
            &config,
            &serde_json::json!({"wp_version": "6.8.3"}),
        )
        .unwrap();

        assert_eq!(wordpress(&config).wp_version.as_deref(), Some("6.8.3"));
    }

    #[test]
    fn test_wordpress_phpix_mode_config() {
        // Python's test_generate_anybuild_wordpress_phpix_mode drives
        // examples/php-wordpress with {"phpix": True}; the resulting plan is
        // pinned by the php-wordpress-empty snapshot (phpix via env there).
        // Here: the example detects as a full wordpress site and the phpix
        // flag survives the user-config merge.
        let example_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/php-wordpress")
            .canonicalize()
            .unwrap();

        let (provider, config) = load_for_path(&example_dir);
        assert_eq!(provider, "wordpress");
        assert!(!wordpress(&config).php.phpix);

        let config =
            merge_config_json("wordpress", &config, &serde_json::json!({"phpix": true})).unwrap();
        let config = wordpress(&config);
        assert!(config.php.phpix);
        // A full site (not an extension) keeps wp_version unset unless the
        // user provides one.
        assert_eq!(config.wp_extension_kind, None);
    }
}
