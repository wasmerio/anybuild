from pathlib import Path
from typing import Dict, Optional
import json
import yaml
from pydantic_settings import SettingsConfigDict

from .base import (
    DetectResult,
    DependencySpec,
    Provider,
    _exists,
    MountSpec,
    ServiceSpec,
    VolumeSpec,
    Metadata,
)


class StaticFileMetadata(Metadata):
    model_config = SettingsConfigDict(extra="ignore", env_prefix="SHIPIT_")

    sws_version: Optional[str] = "2.38.0"
    static_dir: Optional[str] = None


class StaticFileProvider:
    config: Optional[dict] = None
    path: Path

    def __init__(self, path: Path, metadata: StaticFileMetadata):
        self.path = path
        self.metadata = metadata

    @classmethod
    def load_metadata(
        cls, path: Path, base_metadata: Metadata
    ) -> StaticFileMetadata:
        if (path / "Staticfile").exists():
            config = None
            try:
                config = yaml.safe_load((path / "Staticfile").read_text())
            except yaml.YAMLError as e:
                print(f"Error loading Staticfile: {e}")
                pass

            if config:
                return StaticFileMetadata(
                    **base_metadata.model_dump(),
                    static_dir=config.get("root"),
                )
        if _exists(path, "public/index.html") or _exists(path, "public/index.htm"):
            return StaticFileMetadata(static_dir="public", **base_metadata.model_dump())

        return StaticFileMetadata(**base_metadata.model_dump())

    @classmethod
    def name(cls) -> str:
        return "staticfile"

    @classmethod
    def detect(
        cls, path: Path, metadata: Metadata
    ) -> Optional[DetectResult]:
        is_python_php_js_project = _exists(
            path, "package.json", "pyproject.toml", "composer.json"
        )
        if _exists(path, "Staticfile"):
            return DetectResult(cls.name(), 50)
        if not is_python_php_js_project:
            if _exists(
                path, "index.html", "index.htm", "public/index.htm", "public/index.html"
            ):
                return DetectResult(cls.name(), 10)
            return DetectResult(cls.name(), 10)
        if metadata.commands.start and metadata.commands.start.startswith(
            "static-web-server "
        ):
            return DetectResult(cls.name(), 70)
        return None

    def serve_name(self) -> Optional[str]:
        return None

    def dependencies(self) -> list[DependencySpec]:
        return [
            DependencySpec(
                "static-web-server",
                var_name="metadata.sws_version",
                use_in_serve=True,
            )
        ]

    def build_steps(self) -> list[str]:
        return [
            'workdir(static_app.path)',
            'copy({}, ".", ignore=[".git"])'.format(
                json.dumps(self.metadata.static_dir or ".")
            ),
        ]

    def prepare_steps(self) -> Optional[list[str]]:
        return None

    def declarations(self) -> Optional[str]:
        return None

    def commands(self) -> Dict[str, str]:
        return {
            "start": '"static-web-server --root={} --log-level=info --port={}".format(static_app.serve_path, PORT)'
        }

    def mounts(self) -> list[MountSpec]:
        return [MountSpec("static_app")]

    def volumes(self) -> list[VolumeSpec]:
        return []

    def env(self) -> Optional[Dict[str, str]]:
        return None

    def services(self) -> list[ServiceSpec]:
        return []
