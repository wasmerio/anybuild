from __future__ import annotations

from pathlib import Path
from typing import Dict, Optional

from .base import DetectResult, DependencySpec, Provider, _exists, ServiceSpec, VolumeSpec, CustomCommands
from .staticfile import StaticFileProvider

class HugoProvider(StaticFileProvider):

    @classmethod
    def name(cls) -> str:
        return "hugo"

    @classmethod
    def detect(cls, path: Path, custom_commands: CustomCommands) -> Optional[DetectResult]:
        if _exists(path, "hugo.toml", "hugo.json", "hugo.yaml", "hugo.yml"):
            return DetectResult(cls.name(), 80)
        if (
            _exists(path, "config.toml", "config.json", "config.yaml", "config.yml")
            and _exists(path, "content")
            and (_exists(path, "static") or _exists(path, "themes"))
        ):
            return DetectResult(cls.name(), 40)
        return None

    def serve_name(self) -> str:
        return self.path.name

    def provider_kind(self) -> str:
        return "staticsite"

    def dependencies(self) -> list[DependencySpec]:
        return [
            DependencySpec(
                "hugo",
                env_var="SHIPIT_HUGO_VERSION",
                default_version="0.149.0",
                use_in_build=True,
            ),
            *super().dependencies(),
        ]

    def build_steps(self) -> list[str]:
        return [
            'copy(".", ".", ignore=[".git"])',
            'run("hugo build --destination={}".format(app["build"]), group="build")',
        ]
    
    def services(self) -> list[ServiceSpec]:
        return []

    def volumes(self) -> list[VolumeSpec]:
        return []
