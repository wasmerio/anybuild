from __future__ import annotations

from pathlib import Path
from typing import Dict, Optional

from .base import DetectResult, DependencySpec, Provider, _exists


class StaticFileProvider:
    def name(self) -> str:
        return "staticfile"

    def detect(self, path: Path) -> Optional[DetectResult]:
        if _exists(path, "Staticfile"):
            return DetectResult(self.name(), 50)
        if _exists(path, "index.html") and not _exists(
            path, "package.json", "pyproject.toml", "composer.json"
        ):
            return DetectResult(self.name(), 10)
        return None

    def initialize(self, path: Path) -> None:
        pass

    def serve_name(self, path: Path) -> str:
        return path.name

    def provider_kind(self, path: Path) -> str:
        return "staticfile"

    def build_dependencies(self, path: Path) -> list[DependencySpec]:
        return []

    def serve_dependencies(self, path: Path) -> list[DependencySpec]:
        return [DependencySpec("static-web-server", env_var="SHIPIT_SWS_VERSION")]

    def build_steps(self, path: Path) -> list[str]:
        return ["copy(\".\", \".\", ignore=[\".git\"])" ]

    def prepare_script(self, path: Path) -> Optional[str]:
        return None

    def declarations(self, path: Path) -> Optional[str]:
        return None

    def commands(self, path: Path) -> Dict[str, str]:
        return {"start": '"static-web-server --root {}".format(buildpath("site"))'}

    def assets(self, path: Path) -> Optional[Dict[str, str]]:
        return None

    def mounts(self, path: Path) -> Optional[Dict[str, str]]:
        return None

