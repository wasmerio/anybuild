//! Port of `src/shipit/providers/staticfile.py`.
//!
//! Static site detection, Staticfile/`public/` docroot resolution, and the
//! `_redirects` → sws.toml redirects pipeline. The rendered TOML string
//! must match Python's tomlkit output byte-for-byte (it ends up in plans).

use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::base::{env_bool, env_str, BaseConfig, DetectResult, HasBase};

pub const NAME: &str = "staticfile";

pub const REDIRECTS_SOURCE: &str = "_redirects";
pub const REDIRECT_STATUS_CODES: [i64; 2] = [301, 302];

fn param_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r":([A-Za-z][A-Za-z0-9_]*)").expect("valid regex"))
}

fn source_token_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r":([A-Za-z][A-Za-z0-9_]*)|\*").expect("valid regex"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticFileConfig {
    #[serde(flatten)]
    pub base: BaseConfig,
    pub convert_redirects: bool,
    pub sws_version: Option<String>,
    pub static_dir: Option<String>,
    /// Rendered sws.toml redirects (from a _redirects file), computed at
    /// load time so the Starlark provider stays filesystem-free.
    pub redirects_config: Option<String>,
}

impl Default for StaticFileConfig {
    fn default() -> Self {
        Self {
            base: BaseConfig::default(),
            convert_redirects: true,
            sws_version: Some("2.38.0".to_owned()),
            static_dir: None,
            redirects_config: None,
        }
    }
}

impl HasBase for StaticFileConfig {
    fn base(&self) -> &BaseConfig {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseConfig {
        &mut self.base
    }
}

impl StaticFileConfig {
    /// pydantic-settings construction with only the base fields passed as
    /// init kwargs: the env reaches every staticfile-specific field.
    pub(crate) fn from_env(base: BaseConfig) -> Result<Self> {
        Ok(Self {
            base,
            convert_redirects: env_bool("convert_redirects")?.unwrap_or(true),
            sws_version: env_str("sws_version").or_else(|| Some("2.38.0".to_owned())),
            static_dir: env_str("static_dir"),
            redirects_config: env_str("redirects_config"),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RedirectRule {
    pub source: String,
    pub destination: String,
    pub kind: i64,
}

fn exists(path: &Path, candidates: &[&str]) -> bool {
    candidates.iter().any(|c| path.join(c).exists())
}

/// Minimal `yaml.safe_load` replacement for the flat `key: value` YAML
/// documents Shipit reads (Staticfile roots, Jekyll destinations). Only
/// top-level scalar mappings are captured.
pub(crate) fn parse_simple_yaml(contents: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in contents.lines() {
        if line.starts_with([' ', '\t']) {
            continue; // nested value; the flat lookups never need them
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('-') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let mut value = value.trim();
        // Drop trailing comments (approximation of YAML's rules).
        if let Some(index) = value.find(" #") {
            value = value[..index].trim_end();
        }
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);
        if key.is_empty() || value.is_empty() {
            continue;
        }
        map.insert(key.to_owned(), value.to_owned());
    }
    map
}

/// Port of `StaticFileProvider.load_config`.
pub fn load_config(path: &Path, base: BaseConfig) -> Result<StaticFileConfig> {
    let mut config = load_static_config(path, base)?;
    config.redirects_config =
        compute_redirects_config(path, config.static_dir.as_deref(), config.convert_redirects)?;
    Ok(config)
}

/// Port of `StaticFileProvider._load_static_config`.
fn load_static_config(path: &Path, base: BaseConfig) -> Result<StaticFileConfig> {
    let staticfile_path = path.join("Staticfile");
    if staticfile_path.exists() {
        if let Ok(contents) = std::fs::read_to_string(&staticfile_path) {
            let parsed = parse_simple_yaml(&contents);
            if !parsed.is_empty() {
                // static_dir is an explicit init kwarg here (even when the
                // Staticfile has no root), so the env never overrides it.
                let mut config = StaticFileConfig::from_env(base)?;
                config.static_dir = parsed.get("root").cloned();
                return Ok(config);
            }
        }
    }
    if exists(path, &["public/index.html"]) || exists(path, &["public/index.htm"]) {
        let mut config = StaticFileConfig::from_env(base)?;
        config.static_dir = Some("public".to_owned());
        return Ok(config);
    }
    StaticFileConfig::from_env(base)
}

/// Port of `StaticFileProvider.detect`.
pub fn detect(path: &Path, base: &BaseConfig) -> Option<DetectResult> {
    let is_python_php_js_project =
        exists(path, &["package.json", "pyproject.toml", "composer.json"]);
    if exists(path, &["Staticfile"]) {
        return Some(DetectResult {
            name: NAME,
            score: 50,
        });
    }
    if !is_python_php_js_project {
        // Python returns 10 whether or not an index file exists.
        return Some(DetectResult {
            name: NAME,
            score: 10,
        });
    }
    if base
        .commands
        .start
        .as_deref()
        .is_some_and(|start| start.starts_with("static-web-server "))
    {
        return Some(DetectResult {
            name: NAME,
            score: 70,
        });
    }
    None
}

/// Port of `compute_redirects_config`: render a `_redirects` file into
/// sws.toml redirect rules (or None).
pub fn compute_redirects_config(
    path: &Path,
    static_dir: Option<&str>,
    convert_redirects: bool,
) -> Result<Option<String>> {
    if !convert_redirects {
        return Ok(None);
    }

    let mut redirects_path = path.join(REDIRECTS_SOURCE);
    if let Some(static_dir) = static_dir {
        let static_dir_redirects = path.join(static_dir).join(REDIRECTS_SOURCE);
        if static_dir_redirects.is_file() {
            redirects_path = static_dir_redirects;
        }
    }
    if !redirects_path.is_file() {
        return Ok(None);
    }

    let rules = load_redirect_rules(&redirects_path)?;
    if rules.is_empty() {
        return Ok(None);
    }

    // tomlkit renders the [advanced] super-table implicitly: only the
    // [[advanced.redirects]] blocks appear, separated by blank lines.
    let mut rendered = String::new();
    for (index, rule) in rules.iter().enumerate() {
        if index > 0 {
            rendered.push('\n');
        }
        rendered.push_str("[[advanced.redirects]]\n");
        rendered.push_str(&format!("source = {}\n", toml_basic_string(&rule.source)));
        rendered.push_str(&format!(
            "destination = {}\n",
            toml_basic_string(&rule.destination)
        ));
        rendered.push_str(&format!("kind = {}\n", rule.kind));
    }
    Ok(Some(rendered))
}

/// tomlkit-style basic string rendering.
fn toml_basic_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Port of `StaticFileProvider._load_redirect_rules`.
pub(crate) fn load_redirect_rules(redirects_path: &Path) -> Result<Vec<RedirectRule>> {
    let contents = std::fs::read_to_string(redirects_path)
        .with_context(|| format!("{}: unreadable _redirects file", redirects_path.display()))?;
    let mut rules = Vec::new();
    for (index, raw_line) in contents.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parts = shlex_split(line).ok_or_else(|| {
            anyhow!(
                "{}:{line_number}: invalid _redirects rule",
                redirects_path.display()
            )
        })?;
        if parts.len() < 2 {
            bail!(
                "{}:{line_number}: expected source and destination",
                redirects_path.display()
            );
        }

        let source = &parts[0];
        let destination = &parts[1];
        let mut rest = &parts[2..];
        let mut kind: i64 = 301;
        if let Some(first) = rest.first() {
            if !first.is_empty() && first.chars().all(|c| c.is_ascii_digit()) {
                kind = first.parse().map_err(|_| {
                    anyhow!(
                        "{}:{line_number}: redirect status {first} is not supported by \
                         static-web-server",
                        redirects_path.display()
                    )
                })?;
                rest = &rest[1..];
            }
        }

        if !REDIRECT_STATUS_CODES.contains(&kind) {
            bail!(
                "{}:{line_number}: redirect status {kind} is not supported by \
                 static-web-server",
                redirects_path.display()
            );
        }
        if !rest.is_empty() {
            bail!(
                "{}:{line_number}: conditions and forced redirects are not supported",
                redirects_path.display()
            );
        }

        let (sws_source, replacements) = translate_source(redirects_path, line_number, source)?;
        let sws_destination =
            translate_destination(redirects_path, line_number, destination, &replacements)?;
        rules.push(RedirectRule {
            source: sws_source,
            destination: sws_destination,
            kind,
        });
    }
    Ok(rules)
}

/// Port of `StaticFileProvider._translate_source`.
fn translate_source(
    redirects_path: &Path,
    line_number: usize,
    source: &str,
) -> Result<(String, HashMap<String, usize>)> {
    if source.contains("://") {
        bail!(
            "{}:{line_number}: redirect sources must be local paths",
            redirects_path.display()
        );
    }
    if source.contains('?') {
        bail!(
            "{}:{line_number}: query matching is not supported",
            redirects_path.display()
        );
    }
    if !source.starts_with('/') {
        bail!(
            "{}:{line_number}: redirect sources must start with '/'",
            redirects_path.display()
        );
    }

    let mut translated = String::new();
    let mut replacements: HashMap<String, usize> = HashMap::new();
    let mut last_index = 0;
    for (next_index, captures) in (1..).zip(source_token_pattern().captures_iter(source)) {
        let whole = captures.get(0).expect("match 0 exists");
        translated.push_str(&source[last_index..whole.start()]);
        if let Some(param) = captures.get(1) {
            let param_name = param.as_str();
            if replacements.contains_key(param_name) {
                bail!(
                    "{}:{line_number}: duplicate source parameter :{param_name}",
                    redirects_path.display()
                );
            }
            replacements.insert(param_name.to_owned(), next_index);
            translated.push_str("{*}");
        } else {
            if replacements.contains_key("splat") {
                bail!(
                    "{}:{line_number}: only one splat segment is supported",
                    redirects_path.display()
                );
            }
            replacements.insert("splat".to_owned(), next_index);
            translated.push_str("{**}");
        }
        last_index = whole.end();
    }

    translated.push_str(&source[last_index..]);
    Ok((translated, replacements))
}

/// Port of `StaticFileProvider._translate_destination`.
fn translate_destination(
    redirects_path: &Path,
    line_number: usize,
    destination: &str,
    replacements: &HashMap<String, usize>,
) -> Result<String> {
    if destination.contains('*') {
        bail!(
            "{}:{line_number}: destination splats must use :splat",
            redirects_path.display()
        );
    }

    let mut translated = String::new();
    let mut last_index = 0;
    for captures in param_pattern().captures_iter(destination) {
        let whole = captures.get(0).expect("match 0 exists");
        let param_name = captures.get(1).expect("group 1 exists").as_str();
        let Some(replacement) = replacements.get(param_name) else {
            bail!(
                "{}:{line_number}: destination references unknown parameter :{param_name}",
                redirects_path.display()
            );
        };
        translated.push_str(&destination[last_index..whole.start()]);
        translated.push_str(&format!("${replacement}"));
        last_index = whole.end();
    }
    translated.push_str(&destination[last_index..]);
    Ok(translated)
}

/// Minimal `shlex.split` (POSIX mode, comments off): whitespace-separated
/// tokens with single/double quotes and backslash escapes. Returns None on
/// unbalanced quotes or a trailing escape (Python raises ValueError).
fn shlex_split(line: &str) -> Option<Vec<String>> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut chars = line.chars();
    while let Some(ch) = chars.next() {
        match ch {
            c if c.is_whitespace() => {
                if in_word {
                    parts.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(inner) => current.push(inner),
                        None => return None,
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            // In double quotes, backslash only escapes the
                            // quote and itself (shlex posix behavior).
                            Some(escaped @ ('"' | '\\')) => current.push(escaped),
                            Some(other) => {
                                current.push('\\');
                                current.push(other);
                            }
                            None => return None,
                        },
                        Some(inner) => current.push(inner),
                        None => return None,
                    }
                }
            }
            '\\' => {
                in_word = true;
                let escaped = chars.next()?;
                current.push(escaped);
            }
            other => {
                in_word = true;
                current.push(other);
            }
        }
    }
    if in_word {
        parts.push(current);
    }
    Some(parts)
}

#[cfg(test)]
mod tests {
    //! Port of `tests/test_staticfile_provider.py`.

    use super::compute_redirects_config;

    #[test]
    fn test_staticfile_redirects_generate_sws_config_from_static_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let static_dir = tmp.path().join("site");
        std::fs::create_dir_all(&static_dir).unwrap();
        std::fs::write(
            static_dir.join("_redirects"),
            "/docs/* /guides/:splat/ 301\n/blog/:slug /posts/:slug 302\n",
        )
        .unwrap();

        assert_eq!(
            compute_redirects_config(tmp.path(), Some("site"), true)
                .unwrap()
                .as_deref(),
            Some(
                "[[advanced.redirects]]\n\
                 source = \"/docs/{**}\"\n\
                 destination = \"/guides/$1/\"\n\
                 kind = 301\n\
                 \n\
                 [[advanced.redirects]]\n\
                 source = \"/blog/{*}\"\n\
                 destination = \"/posts/$1\"\n\
                 kind = 302\n"
            )
        );
    }

    #[test]
    fn test_staticfile_redirects_fall_back_to_project_root() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("site")).unwrap();
        std::fs::write(
            tmp.path().join("_redirects"),
            "/docs/* /guides/:splat/ 301\n",
        )
        .unwrap();

        assert_eq!(
            compute_redirects_config(tmp.path(), Some("site"), true)
                .unwrap()
                .as_deref(),
            Some(
                "[[advanced.redirects]]\n\
                 source = \"/docs/{**}\"\n\
                 destination = \"/guides/$1/\"\n\
                 kind = 301\n"
            )
        );
    }

    #[test]
    fn test_staticfile_redirects_reject_unsupported_conditions() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("_redirects"),
            "/docs/* /guides/:splat/ 301 Country=us\n",
        )
        .unwrap();

        let error = compute_redirects_config(tmp.path(), None, true).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("conditions and forced redirects are not supported"),
            "{error:#}"
        );
    }
}
