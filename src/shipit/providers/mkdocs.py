from __future__ import annotations

from pathlib import Path
from typing import Dict, Optional

from .base import DetectResult, DependencySpec, Provider, _exists, MountSpec


class MkdocsProvider:
    def __init__(self, path: Path):
        self.path = path
    @classmethod
    def name(cls) -> str:
        return "mkdocs"

    @classmethod
    def detect(cls, path: Path) -> Optional[DetectResult]:
        if _exists(path, "mkdocs.yml", "mkdocs.yaml"):
            return DetectResult(cls.name(), 85)
        return None

    def initialize(self) -> None:
        pass

    def serve_name(self) -> str:
        return self.path.name

    def provider_kind(self) -> str:
        return "mkdocs-site"

    def dependencies(self) -> list[DependencySpec]:
        return [
            DependencySpec(
                "python",
                env_var="SHIPIT_PYTHON_VERSION",
                default_version="3.13",
                use_in_build=True,
            ),
            DependencySpec(
                "uv",
                env_var="SHIPIT_UV_VERSION",
                default_version="0.8.15",
                use_in_build=True,
            ),
            DependencySpec(
                "static-web-server",
                env_var="SHIPIT_SWS_VERSION",
                default_version="2.38.0",
                use_in_serve=True,
            ),
        ]

    def declarations(self) -> Optional[str]:
        return None

    def build_steps(self) -> list[str]:
        has_requirements = _exists(self.path, "requirements.txt")
        if has_requirements:
            install_lines = [
                "run(\"uv init --no-managed-python\", inputs=[], outputs=[\".\"], group=\"install\")",
                "run(f\"uv add -r requirements.txt\", inputs=[\"requirements.txt\"], outputs=[\".venv\"], group=\"install\")",
            ]
        else:
            install_lines = [
                "mkdocs_version = getenv(\"SHIPIT_MKDOCS_VERSION\") or \"1.6.1\"",
                "run(\"uv init --no-managed-python\", inputs=[], outputs=[\".venv\"], group=\"install\")",
                "run(f\"uv add mkdocs=={mkdocs_version}\", group=\"install\")",
            ]
        return [
            *install_lines,
            "copy(\".\", \".\", ignore=[\".venv\", \".git\", \"__pycache__\"])",
            "run(\"uv run mkdocs build --site-dir={}\".format(app[\"build\"]), outputs=[\".\"], group=\"build\")",
        ]

    def prepare_steps(self) -> Optional[list[str]]:
        return None

    def commands(self) -> Dict[str, str]:
        return {"start": '"static-web-server --root /app"'}

    def assets(self) -> Optional[Dict[str, str]]:
        return None

    def mounts(self) -> list[MountSpec]:
        return [MountSpec("app")]

    def env(self) -> Optional[Dict[str, str]]:
        return None
