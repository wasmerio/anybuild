from __future__ import annotations

from pathlib import Path
from typing import Dict, Optional

from .base import DetectResult, DependencySpec, Provider, _exists, _has_dependency, MountSpec


class GatsbyProvider:
    def name(self) -> str:
        return "gatsby"

    def detect(self, path: Path) -> Optional[DetectResult]:
        pkg = path / "package.json"
        if not pkg.exists():
            return None
        if _exists(path, "gatsby-config.js", "gatsby-config.ts") or _has_dependency(
            pkg, "gatsby"
        ):
            return DetectResult(self.name(), 90)
        return None

    def initialize(self, path: Path) -> None:
        pass

    def serve_name(self, path: Path) -> str:
        return path.name

    def provider_kind(self, path: Path) -> str:
        return "staticsite"

    def declarations(self, path: Path) -> Optional[str]:
        return None

    def dependencies(self, path: Path) -> list[DependencySpec]:
        return [
            DependencySpec(
                "node",
                env_var="SHIPIT_NODE_VERSION",
                default_version="22",
                use_in_build=True,
            ),
            DependencySpec("npm", use_in_build=True),
            DependencySpec("static-web-server", env_var="SHIPIT_SWS_VERSION", use_in_serve=True),
        ]

    def build_steps(self, path: Path) -> list[str]:
        return [
            "run(\"npm install\", inputs=[\"package.json\", \"package-lock.json\"], group=\"install\")",
            "copy(\".\", \".\", ignore=[\"node_modules\", \".git\"])",
            "run(\"npm run build\", outputs=[\"public\"], group=\"build\")",
            "run(\"cp -R public/* {}/\".format(app[\"build\"]))",
        ]

    def prepare_steps(self, path: Path) -> Optional[list[str]]:
        return None

    def commands(self, path: Path) -> Dict[str, str]:
        return {"start": '"static-web-server --root /app"'}

    def assets(self, path: Path) -> Optional[Dict[str, str]]:
        return None

    def mounts(self, path: Path) -> list[MountSpec]:
        return [MountSpec("app")]

    def env(self, path: Path) -> Optional[Dict[str, str]]:
        return None
