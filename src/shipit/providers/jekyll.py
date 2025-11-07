from __future__ import annotations

from pathlib import Path
from typing import Dict, Optional

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
from .staticfile import StaticFileProvider


class JekyllProvider(StaticFileProvider):
    def __init__(self, path: Path, custom_commands: CustomCommands):
        self.path = path

    @classmethod
    def name(cls) -> str:
        return "jekyll"

    @classmethod
    def detect(
        cls, path: Path, custom_commands: CustomCommands
    ) -> Optional[DetectResult]:
        if _exists(path, "_config.yml", "_config.yaml"):
            if _exists(path, "Gemfile"):
                return DetectResult(cls.name(), 85)
            return DetectResult(cls.name(), 40)
        if custom_commands.build and custom_commands.build.startswith("jekyll "):
            return DetectResult(cls.name(), 85)
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
                "ruby",
                env_var="SHIPIT_RUBY_VERSION",
                use_in_build=True,
                use_in_serve=False,
            ),
            *super().dependencies(),
        ]

    def declarations(self) -> Optional[str]:
        return 'jekyll_version = getenv("SHIPIT_JEKYLL_VERSION") or "1.6.1"\n'

    def build_steps(self) -> list[str]:
        if _exists(self.path, "Gemfile"):
            install_deps = ["Gemfile"]
            install_deps_str = ", ".join([f'"{dep}"' for dep in install_deps])
            install_commands = [
                f'run("bundle install", inputs=[{install_deps_str}], group="build")'
            ]
            if _exists(self.path, "Gemfile.lock"):
                install_commands = [
                    'copy("Gemfile.lock")',
                    *install_commands,
                ]
        else:
            install_commands = ['run("gem install jekyll", group="build")']
        return [
            'workdir(temp["build"])',
            'copy(".", ignore=[".git"])',
            *install_commands,
            'run("jekyll build --destination={}".format(app["build"]), outputs=["."], group="build")',
        ]

    def prepare_steps(self) -> Optional[list[str]]:
        return None

    def mounts(self) -> list[MountSpec]:
        return [MountSpec("temp", attach_to_serve=False), *super().mounts()]

    def volumes(self) -> list[VolumeSpec]:
        return []

    def env(self) -> Optional[Dict[str, str]]:
        return None

    def services(self) -> list[ServiceSpec]:
        return []
