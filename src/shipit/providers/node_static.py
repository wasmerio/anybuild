from __future__ import annotations

from pathlib import Path
from typing import Dict, Optional

from .base import DetectResult, DependencySpec, Provider, _exists, _has_dependency


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
        return "staticsite-node"

    def provider_kind(self, path: Path) -> str:
        return "staticsite"

    def build_dependencies(self, path: Path) -> list[DependencySpec]:
        return [
            DependencySpec("node", env_var="SHIPIT_NODE_VERSION", default_version="22"),
            DependencySpec("npm"),
        ]

    def serve_dependencies(self, path: Path) -> list[DependencySpec]:
        return [DependencySpec("static-web-server")]

    def build_steps(self, path: Path) -> list[str]:
        output_dir = "dist" if (path / "dist").exists() else "public"
        return [
            "use(node, npm)",
            "run(\"npm install\", inputs=[\"package.json\", \"package-lock.json\"], group=\"install\")",
            "copy(\".\", \".\", ignore=[\"node_modules\", \".git\"])",
            f"run(\"npm run build\", outputs=[\"{output_dir}\"], group=\"build\")",
        ]

    def prepare_script(self, path: Path) -> Optional[str]:
        return None

    def commands(self, path: Path) -> Dict[str, str]:
        output_dir = "dist" if (path / "dist").exists() else "public"
        return {"start": f'"static-web-server --root /app/{output_dir}"'}

    def assets(self, path: Path) -> Optional[Dict[str, str]]:
        return None

    def mounts(self, path: Path) -> Optional[Dict[str, str]]:
        return None

