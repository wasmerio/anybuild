from __future__ import annotations

from pathlib import Path
from typing import Dict, Optional

from .base import DetectResult, DependencySpec, Provider, _exists, MountSpec


class StaticFileProvider:
    def __init__(self, path: Path):
        self.path = path

    @classmethod
    def name(cls) -> str:
        return "staticfile"

    @classmethod
    def detect(cls, path: Path) -> Optional[DetectResult]:
        if _exists(path, "Staticfile"):
            return DetectResult(cls.name(), 50)
        if _exists(path, "index.html") and not _exists(
            path, "package.json", "pyproject.toml", "composer.json"
        ):
            return DetectResult(cls.name(), 10)
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

    def assets(self) -> Optional[Dict[str, str]]:
        return None

    def mounts(self) -> list[MountSpec]:
        return [MountSpec("app")]

    def env(self) -> Optional[Dict[str, str]]:
        return None
