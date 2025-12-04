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


class HugoConfig(StaticFileConfig):
    model_config = SettingsConfigDict(extra="ignore", env_prefix="SHIPIT_")

    hugo_version: Optional[str] = "0.149.0"


class HugoProvider(StaticFileProvider):
    def __init__(self, path: Path, config: HugoConfig):
        super().__init__(path, config)

    @classmethod
    def load_config(cls, path: Path, base_config: Config) -> HugoConfig:
        config = super().load_config(path, base_config)
        return HugoConfig(**config.model_dump())

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
            'run("hugo build --destination={}".format(static_app.path), group="build")',
        ]

    def mounts(self) -> list[MountSpec]:
        return [MountSpec("temp", attach_to_serve=False), *super().mounts()]

    def services(self) -> list[ServiceSpec]:
        return []

    def volumes(self) -> list[VolumeSpec]:
        return []
