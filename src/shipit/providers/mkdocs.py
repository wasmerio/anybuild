from __future__ import annotations

from pathlib import Path
from typing import Dict, Optional

from .base import (
    DetectResult,
    DependencySpec,
    Provider,
    _exists,
    MountSpec,
    ServiceSpec,
    VolumeSpec,
    CustomCommands,
)
from .staticfile import StaticFileProvider
from .python import PythonProvider


class MkdocsProvider(StaticFileProvider):
    def __init__(self, path: Path, custom_commands: CustomCommands):
        self.path = path
        self.python_provider = PythonProvider(path, custom_commands, only_build=True, extra_dependencies={"mkdocs"})

    @classmethod
    def name(cls) -> str:
        return "mkdocs"

    @classmethod
    def detect(cls, path: Path, custom_commands: CustomCommands) -> Optional[DetectResult]:
        if _exists(path, "mkdocs.yml", "mkdocs.yaml"):
            return DetectResult(cls.name(), 85)
        return None

    def initialize(self) -> None:
        pass

    def serve_name(self) -> str:
        return self.path.name

    def provider_kind(self) -> str:
        return "mkdocs-site"

    def dependencies(self) -> list[DependencySpec]:
        return [
            *self.python_provider.dependencies(),
            *super().dependencies(),
        ]

    def declarations(self) -> Optional[str]:
        return "mkdocs_version = getenv(\"SHIPIT_MKDOCS_VERSION\") or \"1.6.1\"\n" + (self.python_provider.declarations() or "")

    def build_steps(self) -> list[str]:
        return [
            *self.python_provider.build_steps(),
            "run(\"uv run mkdocs build --site-dir={}\".format(app[\"build\"]), outputs=[\".\"], group=\"build\")",
        ]

    def prepare_steps(self) -> Optional[list[str]]:
        return self.python_provider.prepare_steps()

    def mounts(self) -> list[MountSpec]:
        return [MountSpec("app"), *self.python_provider.mounts()]

    def volumes(self) -> list[VolumeSpec]:
        return []

    def env(self) -> Optional[Dict[str, str]]:
        return self.python_provider.env()
    
    def services(self) -> list[ServiceSpec]:
        return []
