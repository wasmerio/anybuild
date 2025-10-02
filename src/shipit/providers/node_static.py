from __future__ import annotations

from pathlib import Path
from typing import Dict, Optional

from .base import (
    DetectResult,
    DependencySpec,
    Provider,
    _exists,
    _has_dependency,
    MountSpec,
    ServiceSpec,
    VolumeSpec,
    CustomCommands,
)


class NodeStaticProvider:
    def __init__(self, path: Path, custom_commands: CustomCommands):
        self.path = path
        self.custom_commands = custom_commands

    @classmethod
    def name(cls) -> str:
        return "node-static"

    @classmethod
    def detect(cls, path: Path, custom_commands: CustomCommands) -> Optional[DetectResult]:
        pkg = path / "package.json"
        if not pkg.exists():
            return None
        static_generators = ["astro", "vite", "next", "nuxt"]
        if any(_has_dependency(pkg, dep) for dep in static_generators):
            return DetectResult(cls.name(), 40)
        return None

    def initialize(self) -> None:
        pass

    def serve_name(self) -> str:
        return self.path.name

    def provider_kind(self) -> str:
        return "staticsite"

    def dependencies(self) -> list[DependencySpec]:
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

    def declarations(self) -> Optional[str]:
        return None

    def build_steps(self) -> list[str]:
        output_dir = "dist" if (self.path / "dist").exists() else "public"
        return [
            "run(\"npm install\", inputs=[\"package.json\", \"package-lock.json\"], group=\"install\")",
            "copy(\".\", \".\", ignore=[\"node_modules\", \".git\"])",
            f"run(\"npm run build\", outputs=[\"{output_dir}\"], group=\"build\")",
            f"run(\"cp -R {output_dir}/* {{}}/\".format(app[\"build\"]))",
        ]

    def prepare_steps(self) -> Optional[list[str]]:
        return None

    def commands(self) -> Dict[str, str]:
        output_dir = "dist" if (self.path / "dist").exists() else "public"
        return {"start": f'"static-web-server --root /app/{output_dir}"'}

    def mounts(self) -> list[MountSpec]:
        return [MountSpec("app")]

    def volumes(self) -> list[VolumeSpec]:
        return []

    def env(self) -> Optional[Dict[str, str]]:
        return None
    
    def services(self) -> list[ServiceSpec]:
        return []
