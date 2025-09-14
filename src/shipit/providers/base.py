from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Protocol


@dataclass
class DetectResult:
    name: str
    score: int  # Higher score wins when multiple providers match


class Provider(Protocol):
    def name(self) -> str: ...
    def detect(self, path: Path) -> Optional[DetectResult]: ...
    def initialize(self, path: Path) -> None: ...
    # Structured plan steps
    def serve_name(self, path: Path) -> str: ...
    def provider_kind(self, path: Path) -> str: ...
    def dependencies(self, path: Path) -> list["DependencySpec"]: ...
    def declarations(self, path: Path) -> Optional[str]: ...
    def build_steps(self, path: Path) -> list[str]: ...
    # Prepare: list of Starlark step calls (currently only run(...))
    def prepare_steps(self, path: Path) -> Optional[List[str]]: ...
    def commands(self, path: Path) -> Dict[str, str]: ...
    def assets(self, path: Path) -> Optional[Dict[str, str]]: ...
    def mounts(self, path: Path) -> List["MountSpec"]: ...
    def env(self, path: Path) -> Optional[Dict[str, str]]: ...


@dataclass
class DependencySpec:
    name: str
    env_var: Optional[str] = None
    default_version: Optional[str] = None
    alias: Optional[str] = None  # Variable name in Shipit plan
    use_in_build: bool = False
    use_in_serve: bool = False


@dataclass
class MountSpec:
    name: str
    attach_to_build: bool = True
    attach_to_serve: bool = True


@dataclass
class ProviderPlan:
    serve_name: str
    provider: str
    mounts: List[MountSpec]
    declarations: Optional[str] = None
    dependencies: List[DependencySpec] = field(default_factory=list)
    build_steps: List[str] = field(default_factory=list)
    prepare: Optional[List[str]] = None
    commands: Dict[str, str] = field(default_factory=dict)
    assets: Optional[Dict[str, str]] = None
    env: Optional[Dict[str, str]] = None


def _exists(path: Path, *candidates: str) -> bool:
    return any((path / c).exists() for c in candidates)


def _has_dependency(pkg_json: Path, dep: str) -> bool:
    try:
        import json

        data = json.loads(pkg_json.read_text())
        for section in ("dependencies", "devDependencies", "peerDependencies"):
            if dep in data.get(section, {}):
                return True
    except Exception:
        return False
    return False
