from __future__ import annotations

from pathlib import Path
from typing import Dict, Optional

from .base import DetectResult, DependencySpec, Provider, _exists
from .staticfile import StaticFileProvider

class HugoProvider(StaticFileProvider):
    static_dir = "public"

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

    def serve_name(self, path: Path) -> str:
        return path.name

    def provider_kind(self, path: Path) -> str:
        return "staticsite"

    def dependencies(self, path: Path) -> list[DependencySpec]:
        return [
            DependencySpec(
                "hugo",
                env_var="SHIPIT_HUGO_VERSION",
                default_version="0.149.0",
                use_in_build=True,
            ),
            *super().dependencies(path),
        ]

    def build_steps(self, path: Path) -> list[str]:
        return [
            'copy(".", ".", ignore=[".git"])',
            'run("hugo build", outputs=["public"], group="build")',
        ]
