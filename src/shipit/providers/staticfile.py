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


class StaticFileProvider:
    def __init__(self, path: Path, custom_commands: CustomCommands):
        self.path = path
        self.custom_commands = custom_commands

    @classmethod
    def name(cls) -> str:
        return "staticfile"

    @classmethod
    def detect(cls, path: Path, custom_commands: CustomCommands) -> Optional[DetectResult]:
        if _exists(path, "Staticfile"):
            return DetectResult(cls.name(), 50)
        if _exists(path, "index.html") and not _exists(
            path, "package.json", "pyproject.toml", "composer.json"
        ):
            return DetectResult(cls.name(), 10)
        if custom_commands.start and custom_commands.start.startswith("static-web-server "):
            return DetectResult(cls.name(), 70)
        return None

    def initialize(self) -> None:
        pass

    def serve_name(self) -> str:
        return self.path.name

    def provider_kind(self) -> str:
        return "staticfile"

    def dependencies(self) -> list[DependencySpec]:
        return [
            DependencySpec(
                "static-web-server",
                env_var="SHIPIT_SWS_VERSION",
                default_version="2.38.0",
                use_in_serve=True,
            )
        ]

    def build_steps(self) -> list[str]:
        return [
            'workdir(app["build"])',
            'copy(".", ".", ignore=[".git"])'
        ]

    def prepare_steps(self) -> Optional[list[str]]:
        return None

    def declarations(self) -> Optional[str]:
        return None

    def commands(self) -> Dict[str, str]:
        return {
            "start": '"static-web-server --root={} --log-level=info".format(app["serve"])'
        }

    def mounts(self) -> list[MountSpec]:
        return [MountSpec("app")]

    def volumes(self) -> list[VolumeSpec]:
        return []

    def env(self) -> Optional[Dict[str, str]]:
        return None
    
    def services(self) -> list[ServiceSpec]:
        return []
