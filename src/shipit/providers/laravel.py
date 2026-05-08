from pathlib import Path
from typing import Dict, Optional

from .base import (
    DetectResult,
    DependencySpec,
    _exists,
    MountSpec,
    ServiceSpec,
    VolumeSpec,
    Config,
)
from .php import PhpConfig, PhpProvider
from .node import NodeConfig, NodeProvider
from pydantic_settings import SettingsConfigDict


class LaravelConfig(PhpConfig, NodeConfig):
    model_config = SettingsConfigDict(extra="ignore", env_prefix="SHIPIT_")


class LaravelProvider(PhpProvider):
    def __init__(self, path: Path, config: LaravelConfig):
        self.path = path
        self.node_provider = NodeProvider(path, config, only_build=True)
        self.config = config

    @classmethod
    def load_config(cls, path: Path, base_config: Config) -> LaravelConfig:
        config = super().load_config(path, base_config)
        config.use_composer = True
        node_config = NodeProvider.load_config(
            path, base_config, infer_start=False
        )
        return LaravelConfig(
            **(
                config.model_dump()
                | node_config.model_dump()
                | base_config.model_dump()
            )
        )

    @classmethod
    def name(cls) -> str:
        return "laravel"

    @classmethod
    def detect(cls, path: Path, config: Config) -> Optional[DetectResult]:
        if _exists(path, "artisan") and _exists(path, "composer.json"):
            return DetectResult(cls.name(), 95)
        return None

    def dependencies(self) -> list[DependencySpec]:
        return [
            *super().dependencies(),
            *self.node_provider.dependencies()
        ]

    def build_steps(self) -> list[str]:
        node_install = list(self.node_provider.build_steps_install())
        node_build = list(self.node_provider.build_steps_build())
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
        return [*super().mounts(), *self.node_provider.mounts()]

    def volumes(self) -> list[VolumeSpec]:
        return []

    def env(self) -> Optional[Dict[str, str]]:
        return super().env()

    def services(self) -> list[ServiceSpec]:
        return []
