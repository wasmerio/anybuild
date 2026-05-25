from pathlib import Path
from typing import Any, Dict, Optional

from pydantic_settings import SettingsConfigDict

from .base import (
    Config,
    DetectResult,
    DependencySpec,
    MountSpec,
    ServiceSpec,
    VolumeSpec,
    _exists,
)
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
        self.only_build = True

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

    def dependencies(self) -> list[DependencySpec]:
        return [
            *super().dependencies(),
            *NodeProvider.dependencies(self),
        ]

    def build_steps(self) -> list[str]:
        node_install = list(NodeProvider.build_steps_install(self))
        node_build = list(NodeProvider.build_steps_build(self))
        return super().build_steps_with_options(
            extra_ignore=["node_modules"],
            after_install=node_install,
            after_build=node_build
        )

    def prepare_steps(self) -> Optional[list[str]]:
        return [
            'workdir(app.serve_path)',
            'run("mkdir -p storage/framework/{sessions,views,cache,testing} storage/logs bootstrap/cache")',
            'run("php artisan config:cache")',
            'run("php artisan event:cache")',
            'run("php artisan route:cache")',
            'run("php artisan view:cache")',
        ]

    def commands(self) -> Dict[str, str]:
        return {
            "start": 'f"php -S localhost:{PORT} -t public"',
            "after_deploy": '"php artisan migrate"',
        }

    def mounts(self) -> list[MountSpec]:
        return [*super().mounts(), *NodeProvider.mounts(self)]

    def volumes(self) -> list[VolumeSpec]:
        return []

    def env(self) -> Optional[Dict[str, str]]:
        return super().env()

    def services(self) -> list[ServiceSpec]:
        return []
