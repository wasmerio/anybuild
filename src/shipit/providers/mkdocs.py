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
    Config,
)
from .staticfile import StaticFileProvider, StaticFileConfig
from .python import PythonProvider, PythonConfig
from pydantic_settings import SettingsConfigDict


class MkdocsConfig(PythonConfig, StaticFileConfig):
    model_config = SettingsConfigDict(extra="ignore", env_prefix="SHIPIT_")

    mkdocs_version: Optional[str] = None


class MkdocsProvider(StaticFileProvider):
    def __init__(self, path: Path, config: MkdocsConfig):
        self.path = path
        self.python_provider = PythonProvider(path, config, only_build=True)

    @classmethod
    def load_config(cls, path: Path, base_config: Config) -> MkdocsConfig:
        python_config = PythonProvider.load_config(
            path, base_config, must_have_deps={"mkdocs"}
        )
        staticfile_config = StaticFileProvider.load_config(path, base_config)

        return MkdocsConfig(
            **(
                python_config.model_dump()
                | staticfile_config.model_dump()
                | base_config.model_dump()
            )
        )

    @classmethod
    def name(cls) -> str:
        return "mkdocs"

    @classmethod
    def detect(cls, path: Path, config: Config) -> Optional[DetectResult]:
        if _exists(path, "mkdocs.yml", "mkdocs.yaml"):
            return DetectResult(cls.name(), 85)
        if config.commands.build and config.commands.build.startswith("mkdocs "):
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
        return self.python_provider.declarations() or ""

    def build_steps(self) -> list[str]:
        return [
            *self.python_provider.build_steps(),
            'run("uv run mkdocs build --site-dir={}".format(static_app.path), outputs=["."], group="build")',
        ]

    def prepare_steps(self) -> Optional[list[str]]:
        return self.python_provider.prepare_steps()

    def mounts(self) -> list[MountSpec]:
        return [*self.python_provider.mounts(), *super().mounts()]

    def volumes(self) -> list[VolumeSpec]:
        return []

    def env(self) -> Optional[Dict[str, str]]:
        return self.python_provider.env()

    def services(self) -> list[ServiceSpec]:
        return []
