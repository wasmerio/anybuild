//! Port of the Python-side discovery in `providers/install_context.py`:
//! `discover_python_install_context`, `discover_python_dependency_files`
//! and their helpers (pyproject parsing, uv workspace member globbing,
//! requirements includes walking, requires-all-files logic).
//!
//! The JS-side discovery (`discover_js_install_context`) lives in
//! `node.rs`.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

const REQ_INCLUDE_FLAGS: [&str; 4] = ["-r", "--requirement", "-c", "--constraint"];
const REQ_INLINE_INCLUDE_PREFIXES: [&str; 4] = ["-r", "-c", "--requirement=", "--constraint="];

/// `@ <ref>` in a PEP 508 dependency string (direct references).
static PYTHON_DEPENDENCY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"@\s*([^;\s]+)").unwrap());

/// Port of the `InstallContext` dataclass.
#[derive(Debug, Clone, Default)]
pub struct InstallContext {
    pub inputs: Vec<String>,
    pub local_paths: Vec<PathBuf>,
    pub manifest_paths: Vec<PathBuf>,
    pub requires_all_files: bool,
    pub reasons: Vec<String>,
}

impl InstallContext {
    pub fn add_input(&mut self, value: &str) {
        let value = clean_relative(value);
        if !self.inputs.contains(&value) {
            self.inputs.push(value);
        }
    }

    pub fn add_manifest(&mut self, path: &Path) {
        let resolved = resolve_non_strict(path);
        if !self.manifest_paths.contains(&resolved) {
            self.manifest_paths.push(resolved);
        }
    }

    pub fn add_local_path(
        &mut self,
        path: &Path,
        root: &Path,
        reason: &str,
        require_all_files: bool,
    ) {
        let resolved = resolve_non_strict(path);
        if !self.local_paths.contains(&resolved) {
            self.local_paths.push(resolved.clone());
        }

        if let Some(relative) = relative_to_root(&resolved, root) {
            if looks_like_file_dependency(&resolved) {
                self.add_input(&relative);
            } else if require_all_files {
                self.requires_all_files = true;
                self.add_reason(reason);
            }
            return;
        }

        if require_all_files || !looks_like_file_dependency(&resolved) {
            self.requires_all_files = true;
            self.add_reason(reason);
        }
    }

    fn add_reason(&mut self, reason: &str) {
        if !self.reasons.iter().any(|r| r == reason) {
            self.reasons.push(reason.to_owned());
        }
    }
}

/// Port of `discover_python_install_context`.
pub fn discover_python_install_context(
    root: &Path,
    include_pyproject: bool,
    include_requirements: bool,
) -> InstallContext {
    let root = resolve_non_strict(root);
    let mut context = InstallContext::default();

    if include_pyproject {
        discover_python_pyproject(&root, &mut context);
    }

    if include_requirements {
        let requirements_path = root.join("requirements.txt");
        if requirements_path.exists() {
            let mut visited = HashSet::new();
            visit_requirements_file(&root, &requirements_path, &mut context, &mut visited);
        }
    }

    context
}

/// Port of `discover_python_dependency_files`.
pub fn discover_python_dependency_files(root: &Path) -> Vec<PathBuf> {
    discover_python_install_context(root, true, true)
        .manifest_paths
        .into_iter()
        .filter(|path| path.is_file())
        .collect()
}

/// Port of `_discover_python_pyproject`.
fn discover_python_pyproject(root: &Path, context: &mut InstallContext) {
    let pyproject_path = root.join("pyproject.toml");
    if !pyproject_path.exists() {
        return;
    }

    context.add_input("pyproject.toml");
    context.add_manifest(&pyproject_path);

    let uv_lock_path = root.join("uv.lock");
    if uv_lock_path.exists() {
        context.add_input("uv.lock");
        context.add_manifest(&uv_lock_path);
    }

    for prefix in ["README", "LICENSE", "LICENCE", "MAINTAINERS", "AUTHORS"] {
        for path in glob_prefix_sorted(root, prefix) {
            if let Some(relative) = relative_to_root(&path, root) {
                context.add_input(&relative);
            }
        }
    }

    let Ok(text) = std::fs::read_to_string(&pyproject_path) else {
        return;
    };
    let Ok(data) = text.parse::<toml::Value>() else {
        return;
    };

    let pyproject_dir = pyproject_path.parent().unwrap_or(root).to_path_buf();
    for dependency in python_dependency_strings(&data) {
        if let Some(local_ref) = python_local_ref(&dependency, &pyproject_dir) {
            context.add_local_path(
                &local_ref,
                root,
                "Python local path dependencies need source files",
                true,
            );
        }
    }

    let uv = data
        .get("tool")
        .and_then(|v| v.as_table())
        .and_then(|tool| tool.get("uv"))
        .and_then(|v| v.as_table());

    if let Some(sources) = uv
        .and_then(|uv| uv.get("sources"))
        .and_then(|v| v.as_table())
    {
        for source in sources.values() {
            let Some(source) = source.as_table() else {
                continue;
            };
            let Some(path_value) = source.get("path").and_then(|v| v.as_str()) else {
                continue;
            };
            if let Some(local_ref) = resolve_local_ref(&pyproject_dir, path_value, true) {
                context.add_local_path(&local_ref, root, "uv path sources need source files", true);
            }
        }
    }

    if let Some(members) = uv
        .and_then(|uv| uv.get("workspace"))
        .and_then(|v| v.as_table())
        .and_then(|workspace| workspace.get("members"))
        .and_then(|v| v.as_array())
    {
        for member in members {
            let Some(member) = member.as_str() else {
                continue;
            };
            for member_path in glob_paths(root, member) {
                if member_path.join("pyproject.toml").exists() {
                    context.add_local_path(
                        &member_path,
                        root,
                        "uv workspace members need source files",
                        true,
                    );
                }
            }
        }
    }
}

/// Port of `_visit_requirements_file`.
fn visit_requirements_file(
    root: &Path,
    path: &Path,
    context: &mut InstallContext,
    visited: &mut HashSet<PathBuf>,
) {
    let resolved = resolve_non_strict(path);
    if !visited.insert(resolved.clone()) {
        return;
    }

    if let Some(relative) = relative_to_root(&resolved, root) {
        context.add_input(&relative);
    }

    context.add_manifest(&resolved);
    if !resolved.exists() {
        return;
    }

    let Ok(bytes) = std::fs::read(&resolved) else {
        return;
    };
    let contents = String::from_utf8_lossy(&bytes);
    let parent = resolved
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("/"));

    for line in contents.lines() {
        let tokens = split_requirement_line(line);
        if tokens.is_empty() {
            continue;
        }

        for include_ref in requirement_include_refs(&tokens) {
            let Some(local_ref) = resolve_requirement_file_ref(&parent, &include_ref) else {
                continue;
            };
            visit_requirements_file(root, &local_ref, context, visited);
        }

        if let Some(local_ref) = requirement_local_ref(&tokens, &parent) {
            context.add_local_path(
                &local_ref,
                root,
                "Python local path dependencies need source files",
                true,
            );
        }
    }
}

/// Port of `_python_dependency_strings`.
fn python_dependency_strings(data: &toml::Value) -> Vec<String> {
    let mut dependencies = Vec::new();

    if let Some(project) = data.get("project").and_then(|v| v.as_table()) {
        if let Some(value) = project.get("dependencies") {
            dependencies.extend(string_list(value));
        }
        if let Some(optional) = project
            .get("optional-dependencies")
            .and_then(|v| v.as_table())
        {
            for value in optional.values() {
                dependencies.extend(string_list(value));
            }
        }
    }

    if let Some(groups) = data.get("dependency-groups").and_then(|v| v.as_table()) {
        for value in groups.values() {
            collect_strings(value, &mut dependencies);
        }
    }

    dependencies
}

/// Port of `_python_local_ref`.
fn python_local_ref(value: &str, base_dir: &Path) -> Option<PathBuf> {
    if let Some(captures) = PYTHON_DEPENDENCY_RE.captures(value) {
        return resolve_local_ref(base_dir, captures.get(1).unwrap().as_str(), true);
    }
    resolve_local_ref(base_dir, value, false)
}

/// Port of `_split_requirement_line` (shlex.split with comments, falling
/// back to whitespace splitting on unbalanced quotes).
fn split_requirement_line(line: &str) -> Vec<String> {
    let stripped = line.trim();
    if stripped.is_empty() || stripped.starts_with('#') {
        return Vec::new();
    }
    match shlex_split(stripped) {
        Ok(tokens) => tokens,
        Err(()) => stripped.split_whitespace().map(str::to_owned).collect(),
    }
}

/// Minimal POSIX shlex with `#` comments (mirror of
/// `shlex.split(line, comments=True)` for single-line input).
fn shlex_split(input: &str) -> Result<Vec<String>, ()> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut has_token = false;
    let mut chars = input.chars();

    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {
                if has_token {
                    tokens.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            '#' => {
                // Comment: the rest of the line is discarded.
                if has_token {
                    tokens.push(std::mem::take(&mut current));
                }
                return Ok(tokens);
            }
            '\'' => {
                has_token = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(ch) => current.push(ch),
                        None => return Err(()),
                    }
                }
            }
            '"' => {
                has_token = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(ch @ ('"' | '\\' | '$' | '`')) => current.push(ch),
                            Some(ch) => {
                                current.push('\\');
                                current.push(ch);
                            }
                            None => return Err(()),
                        },
                        Some(ch) => current.push(ch),
                        None => return Err(()),
                    }
                }
            }
            '\\' => {
                has_token = true;
                match chars.next() {
                    Some(ch) => current.push(ch),
                    None => return Err(()),
                }
            }
            ch => {
                has_token = true;
                current.push(ch);
            }
        }
    }

    if has_token {
        tokens.push(current);
    }
    Ok(tokens)
}

/// Port of `_requirement_include_refs`.
fn requirement_include_refs(tokens: &[String]) -> Vec<String> {
    let mut refs = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if REQ_INCLUDE_FLAGS.contains(&token.as_str()) && index + 1 < tokens.len() {
            refs.push(tokens[index + 1].clone());
            index += 2;
            continue;
        }
        for prefix in REQ_INLINE_INCLUDE_PREFIXES {
            if token.starts_with(prefix) && token != prefix {
                refs.push(token[prefix.len()..].to_owned());
                break;
            }
        }
        index += 1;
    }
    refs
}

/// Port of `_requirement_local_ref`.
fn requirement_local_ref(tokens: &[String], base_dir: &Path) -> Option<PathBuf> {
    if REQ_INCLUDE_FLAGS.contains(&tokens[0].as_str()) {
        return None;
    }
    if (tokens[0] == "-e" || tokens[0] == "--editable") && tokens.len() > 1 {
        return resolve_local_ref(base_dir, &tokens[1], false);
    }
    if let Some(rest) = tokens[0].strip_prefix("--editable=") {
        return resolve_local_ref(base_dir, rest, false);
    }

    let joined = tokens.join(" ");
    if let Some(captures) = PYTHON_DEPENDENCY_RE.captures(&joined) {
        return resolve_local_ref(base_dir, captures.get(1).unwrap().as_str(), false);
    }

    if tokens.len() == 1 {
        if let Some(local_ref) = resolve_local_ref(base_dir, &tokens[0], false) {
            return Some(local_ref);
        }
        if tokens[0].contains('/') && !tokens[0].contains("://") {
            return resolve_requirement_file_ref(base_dir, &tokens[0]);
        }
    }
    None
}

/// Port of `_resolve_local_ref`.
fn resolve_local_ref(base_dir: &Path, value: &str, allow_bare_relative: bool) -> Option<PathBuf> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    if let Some(raw_value) = value.strip_prefix("file:") {
        // urlparse always yields scheme "file" here, so take the URL branch.
        let (netloc, path_part) = file_url_parts(raw_value);
        let mut path_value = unquote(&path_part);
        if !netloc.is_empty() && netloc != "localhost" {
            path_value = format!("//{netloc}{path_value}");
        }
        if path_value.starts_with('/') {
            return Some(PathBuf::from(path_value));
        }
        return Some(resolve_non_strict(&base_dir.join(path_value)));
    }
    if let Some(raw_value) = value.strip_prefix("link:") {
        return resolve_local_ref(base_dir, raw_value, allow_bare_relative);
    }

    if value.starts_with("git+file:") {
        return resolve_local_ref(base_dir, &value["git+".len()..], allow_bare_relative);
    }

    if has_url_scheme(value) {
        return None;
    }

    if !is_path_like(value) && !allow_bare_relative {
        return None;
    }

    let path = Path::new(value);
    if path.is_absolute() {
        return Some(path.to_path_buf());
    }
    Some(resolve_non_strict(&base_dir.join(path)))
}

/// Port of `_resolve_requirement_file_ref`.
fn resolve_requirement_file_ref(base_dir: &Path, value: &str) -> Option<PathBuf> {
    if let Some(local_ref) = resolve_local_ref(base_dir, value, false) {
        return Some(local_ref);
    }
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = Path::new(trimmed);
    if path.is_absolute() {
        return Some(path.to_path_buf());
    }
    Some(resolve_non_strict(&base_dir.join(path)))
}

/// `urlparse("file:<raw>")`: netloc plus the path (fragment, query and
/// params stripped, mirroring urllib).
fn file_url_parts(raw: &str) -> (String, String) {
    let (netloc, rest) = match raw.strip_prefix("//") {
        Some(after) => {
            let end = after.find(['/', '?', '#']).unwrap_or(after.len());
            (after[..end].to_owned(), &after[end..])
        }
        None => (String::new(), raw),
    };
    let rest = rest.split('#').next().unwrap_or("");
    let rest = rest.split('?').next().unwrap_or("");
    // urllib splits `;params` off the last path segment only.
    let path = match rest.rfind('/') {
        Some(slash) => match rest[slash..].find(';') {
            Some(offset) => &rest[..slash + offset],
            None => rest,
        },
        None => match rest.find(';') {
            Some(index) => &rest[..index],
            None => rest,
        },
    };
    (netloc, path.to_owned())
}

/// Percent-decoding (urllib `unquote` with errors="replace").
fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).and_then(|b| (*b as char).to_digit(16));
            let low = bytes.get(index + 2).and_then(|b| (*b as char).to_digit(16));
            if let (Some(high), Some(low)) = (high, low) {
                out.push((high << 4 | low) as u8);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Does `urlparse(value).scheme` come back non-empty?
fn has_url_scheme(value: &str) -> bool {
    let Some(pos) = value.find(':') else {
        return false;
    };
    if pos == 0 {
        return false;
    }
    let scheme = &value[..pos];
    let mut chars = scheme.chars();
    let first = chars.next().unwrap();
    first.is_ascii_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Port of `glob.glob(str(root / pattern))` + resolve + sort (used for
/// uv workspace members). `*` does not match dotfiles, mirroring the
/// glob module.
fn glob_paths(root: &Path, pattern: &str) -> Vec<PathBuf> {
    let mut current = vec![root.to_path_buf()];
    for segment in pattern.split('/') {
        if segment.is_empty() {
            continue;
        }
        let mut next = Vec::new();
        if segment.contains(['*', '?', '[']) {
            let Ok(glob) = globset::GlobBuilder::new(segment)
                .literal_separator(true)
                .build()
            else {
                return Vec::new();
            };
            let matcher = glob.compile_matcher();
            for dir in &current {
                let Ok(entries) = std::fs::read_dir(dir) else {
                    continue;
                };
                let mut names: Vec<String> = entries
                    .filter_map(|entry| entry.ok())
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect();
                names.sort();
                for name in names {
                    if name.starts_with('.') && !segment.starts_with('.') {
                        continue;
                    }
                    if matcher.is_match(&name) {
                        next.push(dir.join(&name));
                    }
                }
            }
        } else {
            for dir in &current {
                let candidate = dir.join(segment);
                if candidate.symlink_metadata().is_ok() {
                    next.push(candidate);
                }
            }
        }
        current = next;
    }
    let mut resolved: Vec<PathBuf> = current.iter().map(|p| resolve_non_strict(p)).collect();
    resolved.sort();
    resolved
}

/// `sorted(root.glob("PREFIX*"))` for the fixed manifest-ish patterns.
fn glob_prefix_sorted(root: &Path, prefix: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(prefix))
        .collect();
    names.sort();
    names.into_iter().map(|name| root.join(name)).collect()
}

fn string_list(value: &toml::Value) -> Vec<String> {
    let Some(array) = value.as_array() else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|item| item.as_str().map(str::to_owned))
        .collect()
}

/// Port of `_collect_strings` (recursively over arrays and tables).
fn collect_strings(value: &toml::Value, out: &mut Vec<String>) {
    match value {
        toml::Value::String(s) => out.push(s.clone()),
        toml::Value::Array(items) => {
            for item in items {
                collect_strings(item, out);
            }
        }
        toml::Value::Table(table) => {
            for item in table.values() {
                collect_strings(item, out);
            }
        }
        _ => {}
    }
}

/// `Path.resolve(strict=False)`: canonicalize the existing prefix
/// (following symlinks) and normalize the rest lexically.
pub(crate) fn resolve_non_strict(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(path),
            Err(_) => path.to_path_buf(),
        }
    };
    let mut resolved = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(prefix) => resolved.push(prefix.as_os_str()),
            Component::RootDir => resolved.push(Component::RootDir.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            Component::Normal(name) => {
                resolved.push(name);
                if let Ok(canonical) = resolved.canonicalize() {
                    resolved = canonical;
                }
            }
        }
    }
    resolved
}

/// Port of `_relative_to_root`.
fn relative_to_root(path: &Path, root: &Path) -> Option<String> {
    let path = resolve_non_strict(path);
    let root = resolve_non_strict(root);
    let relative = path.strip_prefix(&root).ok()?;
    let posix = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/");
    Some(clean_relative(&posix))
}

/// Port of `_clean_relative`.
fn clean_relative(value: &str) -> String {
    let mut value = value.replace('\\', "/").trim().to_owned();
    if let Some(stripped) = value.strip_prefix("./") {
        value = stripped.to_owned();
    }
    if value.is_empty() {
        ".".to_owned()
    } else {
        value
    }
}

/// Port of `_is_path_like`.
fn is_path_like(value: &str) -> bool {
    value.starts_with("./")
        || value.starts_with("../")
        || value.starts_with('/')
        || value.starts_with('~')
        || value == "."
        || value == ".."
}

/// Port of `_looks_like_file_dependency`.
fn looks_like_file_dependency(path: &Path) -> bool {
    if path.exists() {
        return path.is_file();
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if name.ends_with('.') {
        return false;
    }
    let trimmed = name.trim_start_matches('.');
    let suffixes = match trimmed.split_once('.') {
        Some((_, rest)) => format!(".{rest}"),
        None => String::new(),
    };
    matches!(
        suffixes.as_str(),
        ".whl" | ".zip" | ".tar" | ".tar.gz" | ".tgz" | ".tar.bz2" | ".tar.xz"
    )
}

#[cfg(test)]
mod tests {
    //! Port of the Python-side half of `tests/test_install_context.py`.
    //! (The JS-side tests live with `discover_js_install_context` in
    //! `node.rs`.)

    use super::*;

    fn write(path: &Path, contents: &str) {
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn test_python_requirements_context_follows_recursive_includes() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let deps_dir = root.join("deps");
        std::fs::create_dir(&deps_dir).unwrap();
        write(
            &root.join("requirements.txt"),
            "-r deps/base.txt\nflask==3.0.0\n",
        );
        write(
            &deps_dir.join("base.txt"),
            "--constraint ../constraints.txt\nfastapi==0.115.0\n",
        );
        write(&root.join("constraints.txt"), "anyio<5\n");

        let context = discover_python_install_context(root, false, true);

        assert_eq!(
            context.inputs,
            ["requirements.txt", "deps/base.txt", "constraints.txt"]
        );
        assert!(!context.requires_all_files);
    }

    #[test]
    fn test_python_requirements_context_detects_external_local_package() {
        let tmp = tempfile::tempdir().unwrap();
        let app_dir = tmp.path().join("app");
        let shared_dir = tmp.path().join("shared");
        std::fs::create_dir(&app_dir).unwrap();
        std::fs::create_dir(&shared_dir).unwrap();
        write(&app_dir.join("requirements.txt"), "-e ../shared\n");
        write(
            &shared_dir.join("pyproject.toml"),
            "[project]\nname = 'shared'\n",
        );

        let context = discover_python_install_context(&app_dir, false, true);

        assert!(context.requires_all_files);
        // local_paths stores resolved paths (tempdirs may sit behind
        // symlinks, e.g. /var -> /private/var on macOS).
        assert!(context.local_paths.contains(&resolve_non_strict(&shared_dir)));
    }

    /// Python's `test_python_provider_uses_context_for_requirements_inputs`
    /// asserts the `uv add` RunStep's inputs on an evaluated plan; the
    /// decisive provider-level fact is the discovered inputs list (the
    /// plan wiring is covered by the snapshot suite).
    #[test]
    fn test_python_provider_uses_context_for_requirements_inputs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let deps_dir = root.join("deps");
        std::fs::create_dir(&deps_dir).unwrap();
        write(&root.join("requirements.txt"), "-r deps/base.txt\n");
        write(&deps_dir.join("base.txt"), "fastapi==0.115.0\n");

        let context = discover_python_install_context(root, false, true);

        assert_eq!(context.inputs, ["requirements.txt", "deps/base.txt"]);
        assert!(!context.requires_all_files);
    }

    #[test]
    fn test_python_pyproject_context_detects_uv_path_source() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let package_dir = root.join("packages").join("shared");
        std::fs::create_dir_all(&package_dir).unwrap();
        write(
            &package_dir.join("pyproject.toml"),
            "[project]\nname = 'shared'\n",
        );
        write(
            &root.join("pyproject.toml"),
            "[project]\nname = \"app\"\ndependencies = [\"shared\"]\n\n[tool.uv.sources]\nshared = { path = \"packages/shared\" }\n",
        );

        let context = discover_python_install_context(root, true, false);

        assert!(context.requires_all_files);
        assert!(context
            .local_paths
            .contains(&resolve_non_strict(&package_dir)));
    }

    #[test]
    fn test_python_pyproject_context_ignores_remote_direct_url() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(
            &root.join("pyproject.toml"),
            "[project]\nname = \"app\"\ndependencies = [\"shared @ https://example.com/shared.whl\"]\n",
        );

        let context = discover_python_install_context(root, true, false);

        assert!(!context.requires_all_files);
        assert!(context.local_paths.is_empty());
    }
}
