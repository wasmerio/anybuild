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
from .staticfile import StaticFileProvider, StaticFileMetadata
from .python import PythonProvider, PythonMetadata
from pydantic import BaseModel, ConfigDict

class MkdocsMetadata(BaseModel):
    model_config = ConfigDict(extra="ignore")
    default_version: Optional[str] = None
    python: Optional[PythonMetadata] = None
    staticfile: Optional[StaticFileMetadata] = None


class MkdocsProvider(StaticFileProvider):
    def __init__(self, path: Path, metadata: MkdocsMetadata):
        self.path = path
        self.full_metadata = metadata
        self.python_provider = PythonProvider(path, metadata.python, only_build=True)

    @property
    def metadata(self) -> StaticFileMetadata:
        return self.full_metadata.staticfile

    @classmethod
    def load_metadata(cls, path: Path, custom_commands: CustomCommands) -> MkdocsMetadata:
        python_metadata = PythonProvider.load_metadata(path, custom_commands, must_have_deps={"mkdocs"})
        staticfile_metadata = StaticFileProvider.load_metadata(path, custom_commands)
        metadata = MkdocsMetadata(
            python=python_metadata,
            staticfile=staticfile_metadata,
        )
        return metadata

    @classmethod
    def name(cls) -> str:
        return "mkdocs"

    @classmethod
    def detect(
        cls, path: Path, custom_commands: CustomCommands
    ) -> Optional[DetectResult]:
        if _exists(path, "mkdocs.yml", "mkdocs.yaml"):
            return DetectResult(cls.name(), 85)
        if custom_commands.build and custom_commands.build.startswith("mkdocs "):
            return DetectResult(cls.name(), 85)
        return None

    def serve_name(self) -> Optional[str]:
        return None

    def dependencies(self) -> list[DependencySpec]:
        return [
            *self.python_provider.dependencies(),
            *super().dependencies(),
        ]

    def declarations(self) -> Optional[str]:
        return 'mkdocs_version = getenv("SHIPIT_MKDOCS_VERSION") or "1.6.1"\n' + (
            self.python_provider.declarations() or ""
        )

    def build_steps(self) -> list[str]:
        return [
            *self.python_provider.build_steps(),
            'run("uv run mkdocs build --site-dir={}".format(app["build"]), outputs=["."], group="build")',
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
