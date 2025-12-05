from pathlib import Path
from typing import Dict, Optional

from .base import (
    DetectResult,
    DependencySpec,
    Provider,
    _exists,
    ServiceSpec,
    VolumeSpec,
    CustomCommands,
    MountSpec,
    Config,
)
from .staticfile import StaticFileProvider, StaticFileConfig
from pydantic_settings import SettingsConfigDict
import toml
import json
import yaml


class HugoConfig(StaticFileConfig):
    model_config = SettingsConfigDict(extra="ignore", env_prefix="SHIPIT_")

    hugo_version: Optional[str] = "0.149.0"
    static_dir: Optional[str] = "public"


class HugoProvider(StaticFileProvider):
    def __init__(self, path: Path, config: HugoConfig):
        super().__init__(path, config)

    @classmethod
    def load_config(cls, path: Path, base_config: Config) -> HugoConfig:
        config = super().load_config(path, base_config)
        if _exists(path, "hugo.toml"):
            config_dict = toml.load(open(path / "hugo.toml"))
        elif _exists(path, "hugo.json"):
            config_dict = json.load(open(path / "hugo.json"))
        elif _exists(path, "hugo.yaml"):
            config_dict = yaml.safe_load(open(path / "hugo.yaml"))
        elif _exists(path, "hugo.yml"):
            config_dict = yaml.safe_load(open(path / "hugo.yml"))
        else:
            config_dict = {}
        
        config = HugoConfig(**config.model_dump())
        if not config.static_dir:
            hugo_publish_dir = None
            if isinstance(config_dict, dict):
                hugo_publish_dir = config_dict.get("publishDir")
                if not hugo_publish_dir:
                    # Use destination as fallback
                    hugo_publish_dir = config_dict.get("destination")
            hugo_publish_dir = hugo_publish_dir or "public"
            assert isinstance(hugo_publish_dir, str), "publishDir in hugo config must be a string"
            config.static_dir = hugo_publish_dir
        return config

    @classmethod
    def name(cls) -> str:
        return "hugo"

    @classmethod
    def detect(
        cls, path: Path, config: Config
    ) -> Optional[DetectResult]:
        if _exists(path, "hugo.toml", "hugo.json", "hugo.yaml", "hugo.yml"):
            return DetectResult(cls.name(), 80)
        if (
            _exists(path, "config.toml", "config.json", "config.yaml", "config.yml")
            and _exists(path, "content")
            and (_exists(path, "static") or _exists(path, "themes"))
        ):
            return DetectResult(cls.name(), 40)
        if config.commands.build and config.commands.build.startswith("hugo "):
            return DetectResult(cls.name(), 80)
        return None

    def serve_name(self) -> Optional[str]:
        return None

    def dependencies(self) -> list[DependencySpec]:
        return [
            DependencySpec(
                "hugo",
                var_name="config.hugo_version",
                use_in_build=True,
            ),
            *super().dependencies(),
        ]

    def build_steps(self) -> list[str]:
        return [
            'workdir(temp.path)',
            'copy(".", ".", ignore=[".git"])',
            'run("hugo build", group="build")',
            'run("cp -R {}/* {}/".format(config.static_dir, static_app.path))'
        ]

    def mounts(self) -> list[MountSpec]:
        return [MountSpec("temp", attach_to_serve=False), *super().mounts()]

    def services(self) -> list[ServiceSpec]:
        return []

    def volumes(self) -> list[VolumeSpec]:
        return []
