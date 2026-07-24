//! Dependency install-input discovery.
//! `providers/install_context.py` for Python and JavaScript projects.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

pub(crate) type JsonMap = serde_json::Map<String, Value>;

const REQ_INCLUDE_FLAGS: [&str; 4] = ["-r", "--requirement", "-c", "--constraint"];
const REQ_INLINE_INCLUDE_PREFIXES: [&str; 4] = ["-r", "-c", "--requirement=", "--constraint="];

const JS_DEPENDENCY_SECTIONS: [&str; 4] = [
    "dependencies",
    "devDependencies",
    "optionalDependencies",
    "peerDependencies",
];

const JS_LOCAL_DEPENDENCY_REASON: &str = "JavaScript local dependencies need source files";
const JS_WORKSPACE_REASON: &str = "JavaScript workspace packages need source files at install time";

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

/// Port of `discover_js_install_context`.
pub fn discover_js_install_context(root: &Path) -> InstallContext {
    let root = resolve_non_strict(root);
    let mut context = InstallContext::default();
    if !root.join("package.json").exists() {
        return context;
    }

    let workspace_packages = find_js_workspace_packages(&root);
    let mut visited = BTreeSet::new();
    visit_js_package(
        &root,
        &root,
        &mut context,
        &workspace_packages,
        &mut visited,
    );

    let mut package_dirs: Vec<&PathBuf> = workspace_packages.values().collect();
    package_dirs.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
    for package_dir in package_dirs {
        if resolve_non_strict(package_dir) == root {
            continue;
        }
        context.add_local_path(package_dir, &root, JS_WORKSPACE_REASON, true);
        visit_js_package(
            package_dir,
            &root,
            &mut context,
            &workspace_packages,
            &mut visited,
        );
    }

    context
}

fn visit_js_package(
    package_dir: &Path,
    root: &Path,
    context: &mut InstallContext,
    workspace_packages: &HashMap<String, PathBuf>,
    visited: &mut BTreeSet<PathBuf>,
) {
    let package_dir = resolve_non_strict(package_dir);
    if !visited.insert(package_dir.clone()) {
        return;
    }

    let package_json_path = package_dir.join("package.json");
    let Some(package_json) = read_json_object(&package_json_path) else {
        return;
    };

    context.add_manifest(&package_json_path);
    if let Some(relative) = relative_to_root(&package_json_path, root) {
        context.add_input(&relative);
    }

    for section in JS_DEPENDENCY_SECTIONS {
        let Some(Value::Object(dependencies)) = package_json.get(section) else {
            continue;
        };
        for (name, spec) in dependencies {
            let Some(spec) = spec.as_str() else {
                continue;
            };
            let Some(local_path) = js_local_ref(name, spec, &package_dir, workspace_packages)
            else {
                continue;
            };
            context.add_local_path(&local_path, root, JS_LOCAL_DEPENDENCY_REASON, true);
            if local_path.is_dir() {
                visit_js_package(&local_path, root, context, workspace_packages, visited);
            }
        }
    }
}

fn js_local_ref(
    name: &str,
    spec: &str,
    base_dir: &Path,
    workspace_packages: &HashMap<String, PathBuf>,
) -> Option<PathBuf> {
    if let Some(target) = spec.strip_prefix("workspace:") {
        if is_path_like(target) {
            return resolve_local_ref(base_dir, target, true);
        }
        return workspace_packages.get(name).cloned();
    }

    if spec.starts_with("file:") || spec.starts_with("link:") {
        return resolve_local_ref(base_dir, spec, true);
    }
    if is_path_like(spec) {
        return resolve_local_ref(base_dir, spec, false);
    }
    None
}

fn find_js_workspace_packages(root: &Path) -> HashMap<String, PathBuf> {
    let package_json = read_json_object(&root.join("package.json")).unwrap_or_default();
    let mut patterns = package_json_workspace_patterns(&package_json);

    let pnpm_workspace = root.join("pnpm-workspace.yaml");
    if pnpm_workspace.exists() {
        patterns.extend(pnpm_workspace_patterns(&pnpm_workspace));
    }

    let mut package_paths = HashMap::new();
    for pattern in patterns {
        if pattern.starts_with('!') {
            continue;
        }
        for path in glob_paths(root, &pattern) {
            if path
                .components()
                .any(|component| component.as_os_str() == "node_modules")
            {
                continue;
            }
            let Some(package_data) = read_json_object(&path.join("package.json")) else {
                continue;
            };
            if package_data.is_empty() {
                continue;
            }
            if let Some(name) = package_data.get("name").and_then(Value::as_str) {
                if !name.is_empty() {
                    package_paths.insert(name.to_owned(), resolve_non_strict(&path));
                }
            }
        }
    }
    package_paths
}

fn package_json_workspace_patterns(package_json: &JsonMap) -> Vec<String> {
    match package_json.get("workspaces") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        Some(Value::Object(map)) => map
            .get("packages")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn pnpm_workspace_patterns(path: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut patterns = Vec::new();
    let mut in_packages = false;
    for raw_line in text.lines() {
        let line = raw_line.trim_end();
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(item) = trimmed.strip_prefix('-') {
            if in_packages {
                if let Some(value) = yaml_scalar(item.trim()) {
                    if !value.is_empty() {
                        patterns.push(value);
                    }
                }
            }
            continue;
        }
        if line.len() != trimmed.len() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("packages:") {
            let rest = rest.trim();
            in_packages = rest.is_empty();
            if let Some(inner) = rest.strip_prefix('[') {
                let inner = inner.trim_end_matches(']');
                for part in inner.split(',') {
                    if let Some(value) = yaml_scalar(part.trim()) {
                        if !value.is_empty() {
                            patterns.push(value);
                        }
                    }
                }
            }
        } else {
            in_packages = false;
        }
    }
    patterns
}

/// Unquote a simple YAML scalar and drop trailing comments.
pub(crate) fn yaml_scalar(item: &str) -> Option<String> {
    let item = item.trim();
    if let Some(rest) = item.strip_prefix('"') {
        return rest.find('"').map(|index| rest[..index].to_owned());
    }
    if let Some(rest) = item.strip_prefix('\'') {
        return rest.find('\'').map(|index| rest[..index].to_owned());
    }
    let end = item.find(" #").unwrap_or(item.len());
    Some(item[..end].trim().to_owned())
}

pub(crate) fn read_json_object(path: &Path) -> Option<JsonMap> {
    let bytes = std::fs::read(path).ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    match value {
        Value::Object(map) => Some(map),
        _ => None,
    }
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

/// Split the portions of a pip requirements line that affect install-input
/// discovery. Pip comments begin at `#` only at the start of a line or after
/// whitespace, so URL fragments remain part of the requirement.
fn split_requirement_line(line: &str) -> Vec<String> {
    let mut previous_was_whitespace = true;
    let mut end = line.len();
    for (index, ch) in line.char_indices() {
        if ch == '#' && previous_was_whitespace {
            end = index;
            break;
        }
        previous_was_whitespace = ch.is_whitespace();
    }
    line[..end].split_whitespace().map(str::to_owned).collect()
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
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            current = current
                .iter()
                .map(|candidate| candidate.join(".."))
                .collect();
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
            for directory in &current {
                let Ok(entries) = std::fs::read_dir(directory) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.starts_with('.') && !segment.starts_with('.') {
                        continue;
                    }
                    if matcher.is_match(&name) {
                        next.push(directory.join(name));
                    }
                }
            }
        } else {
            next.extend(
                current
                    .iter()
                    .map(|directory| directory.join(segment))
                    .filter(|path| path.symlink_metadata().is_ok()),
            );
        }
        current = next;
    }

    let mut resolved: Vec<PathBuf> = current
        .iter()
        .map(|path| resolve_non_strict(path))
        .collect();
    resolved.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
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
    //! Port of `tests/test_install_context.py`.

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
    fn test_requirement_line_strips_pip_comments() {
        assert_eq!(
            split_requirement_line("-r deps/base.txt # install dependencies"),
            ["-r", "deps/base.txt"]
        );
        assert!(split_requirement_line("  # comment").is_empty());
    }

    #[test]
    fn test_requirement_line_preserves_url_fragments() {
        assert_eq!(
            split_requirement_line("package @ https://example.com/repo.git#subdirectory=package"),
            [
                "package",
                "@",
                "https://example.com/repo.git#subdirectory=package"
            ]
        );
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
        assert!(context
            .local_paths
            .contains(&resolve_non_strict(&shared_dir)));
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

    #[test]
    fn test_js_context_detects_recursive_external_file_dependency() {
        let tmp = tempfile::tempdir().unwrap();
        let app_dir = tmp.path().join("app");
        let shared_dir = tmp.path().join("shared");
        let core_dir = tmp.path().join("core");
        std::fs::create_dir(&app_dir).unwrap();
        std::fs::create_dir(&shared_dir).unwrap();
        std::fs::create_dir(&core_dir).unwrap();
        write(
            &app_dir.join("package.json"),
            "{\"dependencies\": {\"shared\": \"file:../shared\"}}",
        );
        write(
            &shared_dir.join("package.json"),
            "{\"name\": \"shared\", \"dependencies\": {\"core\": \"file:../core\"}}",
        );
        write(&core_dir.join("package.json"), "{\"name\": \"core\"}");

        let context = discover_js_install_context(&app_dir);

        assert!(context.requires_all_files);
        assert_eq!(
            context.local_paths,
            [
                resolve_non_strict(&shared_dir),
                resolve_non_strict(&core_dir)
            ]
        );
    }

    #[test]
    fn test_js_context_detects_in_root_file_dependency() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let package_dir = root.join("packages/shared");
        std::fs::create_dir_all(&package_dir).unwrap();
        write(
            &root.join("package.json"),
            "{\"dependencies\": {\"shared\": \"file:packages/shared\"}}",
        );
        write(&package_dir.join("package.json"), "{\"name\": \"shared\"}");

        let context = discover_js_install_context(root);

        assert!(context.requires_all_files);
        assert_eq!(
            context.inputs,
            ["package.json", "packages/shared/package.json"]
        );
        assert_eq!(context.local_paths, [resolve_non_strict(&package_dir)]);
    }

    #[test]
    fn test_js_context_detects_package_json_workspaces() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let package_dir = root.join("packages/ui");
        std::fs::create_dir_all(&package_dir).unwrap();
        write(
            &root.join("package.json"),
            "{\"workspaces\": [\"packages/*\"]}",
        );
        write(&package_dir.join("package.json"), "{\"name\": \"@app/ui\"}");

        let context = discover_js_install_context(root);

        assert!(context.requires_all_files);
        assert_eq!(context.local_paths, [resolve_non_strict(&package_dir)]);
        assert_eq!(
            context.reasons,
            ["JavaScript workspace packages need source files at install time"]
        );
    }

    #[test]
    fn test_workspace_globs_skip_implicit_dotfiles() {
        let tmp = tempfile::tempdir().unwrap();
        let packages = tmp.path().join("packages");
        std::fs::create_dir_all(packages.join("visible")).unwrap();
        std::fs::create_dir_all(packages.join(".hidden")).unwrap();

        assert_eq!(
            glob_paths(tmp.path(), "packages/*"),
            [resolve_non_strict(&packages.join("visible"))]
        );
        assert_eq!(
            glob_paths(tmp.path(), "packages/.*"),
            [resolve_non_strict(&packages.join(".hidden"))]
        );
    }
}
