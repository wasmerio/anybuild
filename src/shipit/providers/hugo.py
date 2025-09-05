from __future__ import annotations

from pathlib import Path
from typing import Dict, Optional

from .base import DetectResult, DependencySpec, Provider, _exists


class HugoProvider:
    def name(self) -> str:
        return "hugo"

    def detect(self, path: Path) -> Optional[DetectResult]:
        if _exists(path, "hugo.toml", "hugo.json", "hugo.yaml", "hugo.yml"):
            return DetectResult(self.name(), 80)
        if (
            _exists(path, "config.toml", "config.json", "config.yaml", "config.yml")
            and _exists(path, "content")
            and (_exists(path, "static") or _exists(path, "themes"))
        ):
            return DetectResult(self.name(), 40)
        return None

    def initialize(self, path: Path) -> None:
        pass

    def serve_name(self, path: Path) -> str:
        return path.name

    def provider_kind(self, path: Path) -> str:
        return "staticsite"

    def build_dependencies(self, path: Path) -> list[DependencySpec]:
        return [
            DependencySpec(
                "hugo", env_var="SHIPIT_HUGO_VERSION", default_version="0.149.0"
            )
        ]

    def serve_dependencies(self, path: Path) -> list[DependencySpec]:
        return [
            DependencySpec(
                "static-web-server",
                env_var="SHIPIT_SWS_VERSION",
                default_version="2.38.0",
            )
        ]

    def declarations(self, path: Path) -> Optional[str]:
        return None

    def build_steps(self, path: Path) -> list[str]:
        return [
            "use(hugo)",
            'copy(".", ".", ignore=[".git"])',
            'run("hugo build", outputs=["public"], group="build")',
        ]

    def prepare_script(self, path: Path) -> Optional[str]:
        return None

    def commands(self, path: Path) -> Dict[str, str]:
        return {"start": '"static-web-server --root {}".format(buildpath("public"))'}

    def assets(self, path: Path) -> Optional[Dict[str, str]]:
        return None

    def mounts(self, path: Path) -> Optional[Dict[str, str]]:
        return None
