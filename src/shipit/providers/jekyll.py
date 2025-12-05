from pathlib import Path
from typing import Dict, Optional
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
    Config,
)
from .staticfile import StaticFileProvider, StaticFileConfig
from pydantic_settings import SettingsConfigDict


class JekyllConfig(StaticFileConfig):
    model_config = SettingsConfigDict(extra="ignore", env_prefix="SHIPIT_")

    ruby_version: Optional[str] = "3.4.7"
    jekyll_version: Optional[str] = "4.3.0"

    static_dir: Optional[str] = "_site"


class JekyllProvider(StaticFileProvider):
    def __init__(self, path: Path, config: JekyllConfig):
        self.path = path
        self.config = config

    @classmethod
    def load_config(
        cls, path: Path, base_config: Config
    ) -> JekyllConfig:
        config = super().load_config(path, base_config)
        config = JekyllConfig(**config.model_dump())
        if not config.static_dir:
            jekyll_static_dir = None
            if _exists(path, "_config.yml"):
                config_dict = yaml.safe_load(open(path / "_config.yml"))
            elif _exists(path, "_config.yaml"):
                config_dict = yaml.safe_load(open(path / "_config.yaml"))
            else:
                config_dict = {}
            if config_dict and isinstance(config_dict, dict):
                jekyll_static_dir = config_dict.get("destination")
            jekyll_static_dir = jekyll_static_dir or "_site"
            assert isinstance(jekyll_static_dir, str), "destination in Jekyll config must be a string"
            config.static_dir = jekyll_static_dir
        return config

    @classmethod
    def name(cls) -> str:
        return "jekyll"

    @classmethod
    def detect(
        cls, path: Path, config: Config
    ) -> Optional[DetectResult]:
        if _exists(path, "_config.yml", "_config.yaml"):
            if _exists(path, "Gemfile"):
                return DetectResult(cls.name(), 85)
            return DetectResult(cls.name(), 40)
        if config.commands.build and config.commands.build.startswith("jekyll "):
            return DetectResult(cls.name(), 85)
        return None

    def serve_name(self) -> Optional[str]:
        return None

    def dependencies(self) -> list[DependencySpec]:
        return [
            DependencySpec(
                "ruby",
                var_name="config.ruby_version",
                use_in_build=True,
                use_in_serve=False,
            ),
            *super().dependencies(),
        ]

    def build_steps(self) -> list[str]:
        if _exists(self.path, "Gemfile"):
            install_deps = ["Gemfile"]
            install_deps_str = ", ".join([f'"{dep}"' for dep in install_deps])
            install_commands = [
                f'run("bundle install", inputs=[{install_deps_str}], group="install")'
            ]
            if _exists(self.path, "Gemfile.lock"):
                install_commands = [
                    'copy("Gemfile.lock")',
                    *install_commands,
                ]
        else:
            install_commands = [
                'run("bundle init", group="install")',
                'run("bundle add jekyll -v {}".format(config.jekyll_version), group="install")',
            ]
        return [
            'workdir(temp.path)',
            *install_commands,
            'copy(".", ignore=[".git"])',
            'run("jekyll build", group="build")',
            'run("cp -R {}/* {}/".format(config.static_dir, static_app.path))'
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
