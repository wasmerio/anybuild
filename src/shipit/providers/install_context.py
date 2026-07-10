import glob
import json
import re
import shlex
from collections.abc import Mapping
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable, Optional
from urllib.parse import unquote, urlparse

import tomlkit
import yaml


@dataclass
class InstallContext:
    inputs: list[str] = field(default_factory=list)
    local_paths: list[Path] = field(default_factory=list)
    manifest_paths: list[Path] = field(default_factory=list)
    requires_all_files: bool = False
    reasons: list[str] = field(default_factory=list)

    def add_input(self, value: str) -> None:
        value = _clean_relative(value)
        if value not in self.inputs:
            self.inputs.append(value)

    def add_manifest(self, path: Path) -> None:
        resolved = path.resolve(strict=False)
        if resolved not in self.manifest_paths:
            self.manifest_paths.append(resolved)

    def add_local_path(
        self,
        path: Path,
        root: Path,
        *,
        reason: str,
        require_all_files: bool = True,
    ) -> None:
        resolved = path.resolve(strict=False)
        if resolved not in self.local_paths:
            self.local_paths.append(resolved)

        relative = _relative_to_root(resolved, root)
        if relative is not None:
            if _looks_like_file_dependency(resolved):
                self.add_input(relative)
            elif require_all_files:
                self.requires_all_files = True
                self._add_reason(reason)
            return

        if require_all_files or not _looks_like_file_dependency(resolved):
            self.requires_all_files = True
            self._add_reason(reason)

    def _add_reason(self, reason: str) -> None:
        if reason not in self.reasons:
            self.reasons.append(reason)


@dataclass(frozen=True)
class LocalRef:
    path: Path


_REQ_INCLUDE_FLAGS = {
    "-r",
    "--requirement",
    "-c",
    "--constraint",
}
_REQ_INLINE_INCLUDE_PREFIXES = (
    "-r",
    "-c",
    "--requirement=",
    "--constraint=",
)
_PYTHON_DEPENDENCY_RE = re.compile(r"@\s*([^;\s]+)")
_DEPENDENCY_SECTIONS = (
    "dependencies",
    "devDependencies",
    "optionalDependencies",
    "peerDependencies",
)


def discover_python_install_context(
    root: Path,
    *,
    include_pyproject: bool = False,
    include_requirements: bool = False,
) -> InstallContext:
    root = root.resolve(strict=False)
    context = InstallContext()

    if include_pyproject:
        _discover_python_pyproject(root, context)

    if include_requirements:
        requirements_path = root / "requirements.txt"
        if requirements_path.exists():
            _visit_requirements_file(
                root,
                requirements_path,
                context,
                visited=set(),
            )

    return context


def discover_python_dependency_files(root: Path) -> list[Path]:
    context = discover_python_install_context(
        root,
        include_pyproject=True,
        include_requirements=True,
    )
    return [
        path for path in context.manifest_paths
        if path.exists() and path.is_file()
    ]


def discover_js_install_context(root: Path) -> InstallContext:
    root = root.resolve(strict=False)
    context = InstallContext()
    package_json_path = root / "package.json"
    if not package_json_path.exists():
        return context

    workspace_packages = _find_js_workspace_packages(root)
    visited: set[Path] = set()
    _visit_js_package(root, root, context, workspace_packages, visited)

    for package_dir in sorted(workspace_packages.values()):
        if package_dir.resolve(strict=False) == root:
            continue
        context.add_local_path(
            package_dir,
            root,
            reason="JavaScript workspace packages need source files at install time",
        )
        _visit_js_package(
            package_dir,
            root,
            context,
            workspace_packages,
            visited,
        )

    return context


def _discover_python_pyproject(root: Path, context: InstallContext) -> None:
    pyproject_path = root / "pyproject.toml"
    if not pyproject_path.exists():
        return

    context.add_input("pyproject.toml")
    context.add_manifest(pyproject_path)

    uv_lock_path = root / "uv.lock"
    if uv_lock_path.exists():
        context.add_input("uv.lock")
        context.add_manifest(uv_lock_path)

    for pattern in (
        "README*",
        "LICENSE*",
        "LICENCE*",
        "MAINTAINERS*",
        "AUTHORS*",
    ):
        for path in sorted(root.glob(pattern)):
            relative = _relative_to_root(path, root)
            if relative is not None:
                context.add_input(relative)

    try:
        data = tomlkit.parse(pyproject_path.read_text())
    except Exception:
        return

    for dependency in _python_dependency_strings(data):
        local_ref = _python_local_ref(dependency, pyproject_path.parent)
        if local_ref:
            context.add_local_path(
                local_ref.path,
                root,
                reason="Python local path dependencies need source files",
            )

    tool = _mapping_value(data, "tool")
    uv = _mapping_value(tool, "uv")
    sources = _mapping_value(uv, "sources")
    for source in sources.values() if isinstance(sources, Mapping) else ():
        if not isinstance(source, Mapping):
            continue
        path_value = source.get("path")
        if isinstance(path_value, str):
            local_ref = _resolve_local_ref(
                pyproject_path.parent,
                path_value,
                allow_bare_relative=True,
            )
            if local_ref:
                context.add_local_path(
                    local_ref.path,
                    root,
                    reason="uv path sources need source files",
                )

    workspace = _mapping_value(uv, "workspace")
    members = (
        workspace.get("members", [])
        if isinstance(workspace, Mapping)
        else []
    )
    if isinstance(members, list):
        for member in members:
            if not isinstance(member, str):
                continue
            for member_path in _glob_paths(root, member):
                if (member_path / "pyproject.toml").exists():
                    context.add_local_path(
                        member_path,
                        root,
                        reason="uv workspace members need source files",
                    )


def _visit_requirements_file(
    root: Path,
    path: Path,
    context: InstallContext,
    *,
    visited: set[Path],
) -> None:
    resolved = path.resolve(strict=False)
    if resolved in visited:
        return
    visited.add(resolved)

    relative = _relative_to_root(resolved, root)
    if relative is not None:
        context.add_input(relative)

    context.add_manifest(resolved)
    if not resolved.exists():
        return

    for line in resolved.read_text().splitlines():
        tokens = _split_requirement_line(line)
        if not tokens:
            continue

        for include_ref in _requirement_include_refs(tokens):
            local_ref = _resolve_requirement_file_ref(
                resolved.parent,
                include_ref,
            )
            if not local_ref:
                continue
            _visit_requirements_file(
                root,
                local_ref.path,
                context,
                visited=visited,
            )

        local_ref = _requirement_local_ref(tokens, resolved.parent)
        if local_ref:
            context.add_local_path(
                local_ref.path,
                root,
                reason="Python local path dependencies need source files",
            )


def _visit_js_package(
    package_dir: Path,
    root: Path,
    context: InstallContext,
    workspace_packages: dict[str, Path],
    visited: set[Path],
) -> None:
    package_dir = package_dir.resolve(strict=False)
    if package_dir in visited:
        return
    visited.add(package_dir)

    package_json_path = package_dir / "package.json"
    package_json = _read_json(package_json_path)
    if package_json is None:
        return

    context.add_manifest(package_json_path)
    relative = _relative_to_root(package_json_path, root)
    if relative is not None:
        context.add_input(relative)

    for name, spec in _js_dependency_specs(package_json):
        local_ref = _js_local_ref(
            name,
            spec,
            package_dir,
            workspace_packages,
        )
        if local_ref is None:
            continue
        context.add_local_path(
            local_ref.path,
            root,
            reason="JavaScript local dependencies need source files",
        )
        if local_ref.path.is_dir():
            _visit_js_package(
                local_ref.path,
                root,
                context,
                workspace_packages,
                visited,
            )


def _python_dependency_strings(data: Any) -> list[str]:
    dependencies: list[str] = []

    project = _mapping_value(data, "project")
    dependencies.extend(_string_list(project.get("dependencies", [])))
    optional = project.get("optional-dependencies", {})
    if isinstance(optional, Mapping):
        for value in optional.values():
            dependencies.extend(_string_list(value))

    dependency_groups = _mapping_value(data, "dependency-groups")
    if isinstance(dependency_groups, Mapping):
        dependencies.extend(_collect_strings(dependency_groups.values()))

    return dependencies


def _python_local_ref(value: str, base_dir: Path) -> Optional[LocalRef]:
    match = _PYTHON_DEPENDENCY_RE.search(value)
    if match:
        return _resolve_local_ref(
            base_dir,
            match.group(1),
            allow_bare_relative=True,
        )
    return _resolve_local_ref(base_dir, value)


def _split_requirement_line(line: str) -> list[str]:
    stripped = line.strip()
    if not stripped or stripped.startswith("#"):
        return []
    try:
        return shlex.split(stripped, comments=True)
    except ValueError:
        return stripped.split()


def _requirement_include_refs(tokens: list[str]) -> list[str]:
    refs = []
    index = 0
    while index < len(tokens):
        token = tokens[index]
        if token in _REQ_INCLUDE_FLAGS and index + 1 < len(tokens):
            refs.append(tokens[index + 1])
            index += 2
            continue
        for prefix in _REQ_INLINE_INCLUDE_PREFIXES:
            if token.startswith(prefix) and token != prefix:
                refs.append(token[len(prefix):])
                break
        index += 1
    return refs


def _requirement_local_ref(
    tokens: list[str],
    base_dir: Path,
) -> Optional[LocalRef]:
    if tokens[0] in _REQ_INCLUDE_FLAGS:
        return None
    if tokens[0] in ("-e", "--editable") and len(tokens) > 1:
        return _resolve_local_ref(base_dir, tokens[1])
    if tokens[0].startswith("--editable="):
        return _resolve_local_ref(base_dir, tokens[0].split("=", 1)[1])

    joined = " ".join(tokens)
    match = _PYTHON_DEPENDENCY_RE.search(joined)
    if match:
        return _resolve_local_ref(base_dir, match.group(1))

    if len(tokens) == 1:
        local_ref = _resolve_local_ref(base_dir, tokens[0])
        if local_ref:
            return local_ref
        if "/" in tokens[0] and "://" not in tokens[0]:
            return _resolve_requirement_file_ref(base_dir, tokens[0])
    return None


def _js_dependency_specs(package_json: dict[str, Any]) -> Iterable[tuple[str, str]]:
    for section in _DEPENDENCY_SECTIONS:
        deps = package_json.get(section, {})
        if not isinstance(deps, dict):
            continue
        for name, spec in deps.items():
            if isinstance(name, str) and isinstance(spec, str):
                yield name, spec


def _js_local_ref(
    name: str,
    spec: str,
    base_dir: Path,
    workspace_packages: dict[str, Path],
) -> Optional[LocalRef]:
    if spec.startswith("workspace:"):
        target = spec[len("workspace:"):]
        if _is_path_like(target):
            return _resolve_local_ref(
                base_dir,
                target,
                allow_bare_relative=True,
            )
        workspace_path = workspace_packages.get(name)
        if workspace_path:
            return LocalRef(workspace_path)
        return None

    for prefix in ("file:", "link:"):
        if spec.startswith(prefix):
            return _resolve_local_ref(
                base_dir,
                spec,
                allow_bare_relative=True,
            )

    if _is_path_like(spec):
        return _resolve_local_ref(base_dir, spec)
    return None


def _find_js_workspace_packages(root: Path) -> dict[str, Path]:
    package_json = _read_json(root / "package.json") or {}
    patterns = _package_json_workspace_patterns(package_json)

    pnpm_workspace = root / "pnpm-workspace.yaml"
    if pnpm_workspace.exists():
        try:
            data = yaml.safe_load(pnpm_workspace.read_text()) or {}
        except Exception:
            data = {}
        workspace_patterns = (
            data.get("packages", []) if isinstance(data, dict) else []
        )
        if isinstance(workspace_patterns, list):
            patterns.extend(
                pattern for pattern in workspace_patterns
                if isinstance(pattern, str)
            )

    package_paths: dict[str, Path] = {}
    for pattern in patterns:
        if pattern.startswith("!"):
            continue
        for path in _glob_paths(root, pattern):
            if "node_modules" in path.parts:
                continue
            package_json_path = path / "package.json"
            package_data = _read_json(package_json_path)
            if not package_data:
                continue
            name = package_data.get("name")
            if isinstance(name, str) and name:
                package_paths[name] = path.resolve(strict=False)
    return package_paths


def _package_json_workspace_patterns(package_json: dict[str, Any]) -> list[str]:
    workspaces = package_json.get("workspaces", [])
    if isinstance(workspaces, list):
        return [item for item in workspaces if isinstance(item, str)]
    if isinstance(workspaces, dict):
        packages = workspaces.get("packages", [])
        if isinstance(packages, list):
            return [item for item in packages if isinstance(item, str)]
    return []


def _resolve_local_ref(
    base_dir: Path,
    value: str,
    *,
    allow_bare_relative: bool = False,
) -> Optional[LocalRef]:
    value = value.strip()
    if not value:
        return None

    if value.startswith(("file:", "link:")):
        prefix, raw_value = value.split(":", 1)
        if prefix == "file":
            parsed = urlparse(value)
            if parsed.scheme == "file":
                path_value = unquote(parsed.path)
                if parsed.netloc and parsed.netloc != "localhost":
                    path_value = f"//{parsed.netloc}{path_value}"
                if path_value.startswith("/"):
                    return LocalRef(Path(path_value))
                return LocalRef((base_dir / path_value).resolve(strict=False))
        return _resolve_local_ref(
            base_dir,
            raw_value,
            allow_bare_relative=allow_bare_relative,
        )

    if value.startswith("git+file:"):
        return _resolve_local_ref(
            base_dir,
            value[len("git+"):],
            allow_bare_relative=allow_bare_relative,
        )

    parsed = urlparse(value)
    if parsed.scheme:
        return None

    if not _is_path_like(value) and not allow_bare_relative:
        return None

    path = Path(value)
    if path.is_absolute():
        return LocalRef(path)
    return LocalRef((base_dir / path).resolve(strict=False))


def _resolve_requirement_file_ref(
    base_dir: Path,
    value: str,
) -> Optional[LocalRef]:
    local_ref = _resolve_local_ref(base_dir, value)
    if local_ref:
        return local_ref
    path = Path(value.strip())
    if not value.strip():
        return None
    if path.is_absolute():
        return LocalRef(path)
    return LocalRef((base_dir / path).resolve(strict=False))


def _mapping_value(data: Any, key: str) -> Mapping[str, Any]:
    value = data.get(key, {}) if isinstance(data, Mapping) else {}
    return value if isinstance(value, Mapping) else {}


def _string_list(value: Any) -> list[str]:
    if not isinstance(value, list):
        return []
    return [item for item in value if isinstance(item, str)]


def _collect_strings(values: Iterable[Any]) -> list[str]:
    strings: list[str] = []
    for value in values:
        if isinstance(value, str):
            strings.append(value)
        elif isinstance(value, list):
            strings.extend(_collect_strings(value))
        elif isinstance(value, Mapping):
            strings.extend(_collect_strings(value.values()))
    return strings


def _read_json(path: Path) -> Optional[dict[str, Any]]:
    if not path.exists():
        return None
    try:
        data = json.loads(path.read_text())
    except Exception:
        return None
    return data if isinstance(data, dict) else None


def _glob_paths(root: Path, pattern: str) -> list[Path]:
    matches = glob.glob(str(root / pattern))
    return sorted(Path(match).resolve(strict=False) for match in matches)


def _relative_to_root(path: Path, root: Path) -> Optional[str]:
    try:
        relative = path.resolve(strict=False).relative_to(root.resolve(strict=False))
    except ValueError:
        return None
    return _clean_relative(relative.as_posix())


def _clean_relative(value: str) -> str:
    value = value.replace("\\", "/").strip()
    if value.startswith("./"):
        value = value[2:]
    return value or "."


def _is_path_like(value: str) -> bool:
    return (
        value.startswith(("./", "../", "/", "~"))
        or value in {".", ".."}
    )


def _looks_like_file_dependency(path: Path) -> bool:
    if path.exists():
        return path.is_file()
    suffixes = "".join(path.suffixes)
    return suffixes in {
        ".whl",
        ".zip",
        ".tar",
        ".tar.gz",
        ".tgz",
        ".tar.bz2",
        ".tar.xz",
    }
