from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Protocol, Literal


@dataclass
class DetectResult:
    name: str
    score: int  # Higher score wins when multiple providers match


@dataclass
class CustomCommands:
    install: Optional[str] = None
    build: Optional[str] = None
    start: Optional[str] = None
    after_deploy: Optional[str] = None


class Provider(Protocol):
    def __init__(self, path: Path): ...
    @classmethod
    def name(cls) -> str: ...
    @classmethod
    def detect(cls, path: Path, custom_commands: CustomCommands) -> Optional[DetectResult]: ...
    def initialize(self) -> None: ...
    # Structured plan steps (no path args; use self.path)
    def serve_name(self) -> str: ...
    def provider_kind(self) -> str: ...
    def dependencies(self) -> list["DependencySpec"]: ...
    def declarations(self) -> Optional[str]: ...
    def build_steps(self) -> list[str]: ...
    # Prepare: list of Starlark step calls (currently only run(...))
    def prepare_steps(self) -> Optional[List[str]]: ...
    def commands(self) -> Dict[str, str]: ...
    def services(self) -> List["ServiceSpec"]: ...
    def mounts(self) -> List["MountSpec"]: ...
    def volumes(self) -> List["VolumeSpec"]: ...
    def env(self) -> Optional[Dict[str, str]]: ...


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
class VolumeSpec:
    name: str
    # Absolute path inside the serve/runtime environment where the volume is mounted
    serve_path: str
    var_name: Optional[str] = None


@dataclass
class ServiceSpec:
    name: str
    provider: Literal["postgres", "mysql", "redis"]


@dataclass
class ProviderPlan:
    serve_name: str
    provider: str
    mounts: List[MountSpec]
    volumes: List[VolumeSpec] = field(default_factory=list)
    declarations: Optional[str] = None
    dependencies: List[DependencySpec] = field(default_factory=list)
    build_steps: List[str] = field(default_factory=list)
    prepare: Optional[List[str]] = None
    services: List[ServiceSpec] = field(default_factory=list)
    commands: Dict[str, str] = field(default_factory=dict)
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
