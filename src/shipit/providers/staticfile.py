from __future__ import annotations

from pathlib import Path
from typing import Dict, Optional
import json
import yaml

from .base import (
    DetectResult,
    DependencySpec,
    Provider,
    _exists,
    MountSpec,
    ServiceSpec,
    VolumeSpec,
    CustomCommands,
)


class StaticFileProvider:
    config: Optional[dict] = None
    path: Path
    custom_commands: CustomCommands

    subdir: str | None = None

    def __init__(self, path: Path, custom_commands: CustomCommands):
        self.path = path
        self.custom_commands = custom_commands
        if (self.path / "Staticfile").exists():
            try:
                self.config = yaml.safe_load((self.path / "Staticfile").read_text())
            except yaml.YAMLError as e:
                print(f"Error loading Staticfile: {e}")
                pass
        self.subdir = self._determine_subdir()

    def _determine_subdir(self) -> str | None:
        if self.config and "root" in self.config:
            return self.config["root"]
        elif (self.path / "index.html").exists():
            return None
        elif (self.path / "public" / "index.html").exists():
            return "public"
        else:
            return None


    @classmethod
    def name(cls) -> str:
        return "staticfile"

    @classmethod
    def detect(
        cls, path: Path, custom_commands: CustomCommands
    ) -> Optional[DetectResult]:
        if _exists(path, "Staticfile"):
            return DetectResult(cls.name(), 50)

        is_package = _exists(path, "package.json", "pyproject.toml", "composer.json")

        if _exists(path / "public", "index.html") and not is_package:
            return DetectResult(cls.name(), 15)
        if _exists(path, "index.html") and not is_package:
            return DetectResult(cls.name(), 10)
        if custom_commands.start and custom_commands.start.startswith(
            "static-web-server "
        ):
            return DetectResult(cls.name(), 70)

        return None

    def initialize(self) -> None:
        pass

    def serve_name(self) -> Optional[str]:
        return None

    def platform(self) -> Optional[str]:
        return None

    def dependencies(self) -> list[DependencySpec]:
        return [
            DependencySpec(
                "static-web-server",
                env_var="SHIPIT_SWS_VERSION",
                default_version="2.38.0",
                use_in_serve=True,
            )
        ]

    def build_steps(self) -> list[str]:
        return [
            'workdir(app["build"])',
            'copy({}, ".", ignore=[".git"])'.format(
                json.dumps(self.subdir or '.')
            ),
        ]

    def prepare_steps(self) -> Optional[list[str]]:
        return None

    def declarations(self) -> Optional[str]:
        return None

    def commands(self) -> Dict[str, str]:
        root =  'app["serve"]'
        if self.subdir:
            root += f' + "/{self.subdir}"'
        return {
            "start": '"static-web-server --root={} --log-level=info --port={}".format(' + root + ', PORT)',
        }

    def mounts(self) -> list[MountSpec]:
        return [MountSpec("app")]

    def volumes(self) -> list[VolumeSpec]:
        return []

    def env(self) -> Optional[Dict[str, str]]:
        return None

    def services(self) -> list[ServiceSpec]:
        return []
