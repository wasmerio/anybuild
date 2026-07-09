from pathlib import Path
from typing import Optional
import yaml

from .base import DetectResult, _exists, Config
from .staticfile import StaticFileProvider, StaticFileConfig, compute_redirects_config
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
        # static_dir may have changed since the base load; recompute redirects.
        config.redirects_config = compute_redirects_config(
            path, config.static_dir, config.convert_redirects
        )
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

