from pathlib import Path
from typing import Any, Optional

from pydantic_settings import SettingsConfigDict

from .base import Config, DetectResult, _exists
from .node import NodeConfig, NodeProvider
from .php import PhpConfig, PhpProvider


class LaravelConfig(PhpConfig, NodeConfig):
    model_config = SettingsConfigDict(extra="ignore", env_prefix="SHIPIT_")

    framework: Optional[Any] = None


class LaravelProvider(PhpProvider, NodeProvider):
    config: LaravelConfig

    def __init__(self, path: Path, config: LaravelConfig):
        self.path = path
        self.config = config

    @classmethod
    def load_config(
        cls,
        path: Path,
        base_config: Config,
        infer_start: bool = False,
    ) -> LaravelConfig:
        config = super().load_config(path, base_config)
        config.use_composer = True
        node_config = NodeProvider.load_config(
            path, base_config, infer_start=False
        )
        node_config_data = node_config.model_dump(exclude={"framework"})
        return LaravelConfig(
            **(
                config.model_dump()
                | node_config_data
                | base_config.model_dump()
            )
        )

    @classmethod
    def name(cls) -> str:
        return "laravel"

    @classmethod
    def detect_framework(cls, *args: Any, **kwargs: Any) -> Any:
        return PhpProvider.detect_framework(*args, **kwargs)

    @classmethod
    def detect(cls, path: Path, config: Config) -> Optional[DetectResult]:
        if _exists(path, "artisan") and _exists(path, "composer.json"):
            return DetectResult(cls.name(), 95)
        return None

