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


class GatsbyProvider:
    def __init__(self, path: Path, custom_commands: CustomCommands):
        self.path = path
        self.custom_commands = custom_commands

    @classmethod
    def name(cls) -> str:
        return "gatsby"

    @classmethod
    def detect(cls, path: Path, custom_commands: CustomCommands) -> Optional[DetectResult]:
        pkg = path / "package.json"
        if not pkg.exists():
            return None
        if _exists(path, "gatsby-config.js", "gatsby-config.ts") or _has_dependency(
            pkg, "gatsby"
        ):
            return DetectResult(cls.name(), 90)
        return None

    def initialize(self) -> None:
        pass

    def serve_name(self) -> str:
        return self.path.name

    def provider_kind(self) -> str:
        return "staticsite"

    def declarations(self) -> Optional[str]:
        return None

    def dependencies(self) -> list[DependencySpec]:
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

    def build_steps(self) -> list[str]:
        return [
            "run(\"npm install\", inputs=[\"package.json\", \"package-lock.json\"], group=\"install\")",
            "copy(\".\", \".\", ignore=[\"node_modules\", \".git\"])",
            "run(\"npm run build\", outputs=[\"public\"], group=\"build\")",
            "run(\"cp -R public/* {}/\".format(app[\"build\"]))",
        ]

    def prepare_steps(self) -> Optional[list[str]]:
        return None

    def commands(self) -> Dict[str, str]:
        return {"start": '"static-web-server --root /app"'}

    def mounts(self) -> list[MountSpec]:
        return [MountSpec("app")]

    def volumes(self) -> list[VolumeSpec]:
        return []

    def env(self) -> Optional[Dict[str, str]]:
        return None
    
    def services(self) -> list[ServiceSpec]:
        return []
