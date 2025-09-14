from __future__ import annotations

from pathlib import Path
from typing import Dict, Optional

from .base import DetectResult, DependencySpec, Provider, _exists, _has_dependency, MountSpec


class NodeStaticProvider:
    def name(self) -> str:
        return "node-static"

    def detect(self, path: Path) -> Optional[DetectResult]:
        pkg = path / "package.json"
        if not pkg.exists():
            return None
        static_generators = ["astro", "vite", "next", "nuxt"]
        if any(_has_dependency(pkg, dep) for dep in static_generators):
            return DetectResult(self.name(), 40)
        return None

    def initialize(self, path: Path) -> None:
        pass

    def serve_name(self, path: Path) -> str:
        return path.name

    def provider_kind(self, path: Path) -> str:
        return "staticsite"

    def dependencies(self, path: Path) -> list[DependencySpec]:
        return [
            DependencySpec(
                "node",
                env_var="SHIPIT_NODE_VERSION",
                default_version="22",
                use_in_build=True,
            ),
            DependencySpec("npm", use_in_build=True),
            DependencySpec("static-web-server", use_in_serve=True),
        ]

    def declarations(self, path: Path) -> Optional[str]:
        return None

    def build_steps(self, path: Path) -> list[str]:
        output_dir = "dist" if (path / "dist").exists() else "public"
        return [
            "run(\"npm install\", inputs=[\"package.json\", \"package-lock.json\"], group=\"install\")",
            "copy(\".\", \".\", ignore=[\"node_modules\", \".git\"])",
            f"run(\"npm run build\", outputs=[\"{output_dir}\"], group=\"build\")",
            f"run(\"cp -R {output_dir}/* {{}}/\".format(app[\"build\"]))",
        ]

    def prepare_steps(self, path: Path) -> Optional[list[str]]:
        return None

    def commands(self, path: Path) -> Dict[str, str]:
        output_dir = "dist" if (path / "dist").exists() else "public"
        return {"start": f'"static-web-server --root /app/{output_dir}"'}

    def assets(self, path: Path) -> Optional[Dict[str, str]]:
        return None

    def mounts(self, path: Path) -> list[MountSpec]:
        return [MountSpec("app")]

    def env(self, path: Path) -> Optional[Dict[str, str]]:
        return None
