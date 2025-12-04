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
    Config,
)
from .php import PhpConfig, PhpProvider
from .node_static import NodeStaticConfig, NodeStaticProvider
from pydantic_settings import SettingsConfigDict


class LaravelConfig(PhpConfig, NodeStaticConfig):
    model_config = SettingsConfigDict(extra="ignore", env_prefix="SHIPIT_")


class LaravelProvider(PhpProvider):
    def __init__(self, path: Path, config: LaravelConfig):
        self.path = path
        self.node_provider = NodeStaticProvider(path, config, only_build=True)
        self.config = config

    @classmethod
    def load_config(cls, path: Path, base_config: Config) -> LaravelConfig:
        config = super().load_config(path, base_config)
        node_config = NodeStaticProvider.load_config(path, base_config)
        node_config.static_dir = None
        node_config.static_generator = None
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

    def serve_name(self) -> Optional[str]:
        return None

    def dependencies(self) -> list[DependencySpec]:
        return [
            DependencySpec(
                "php",
                var_name="config.php_version",
                use_in_build=True,
                use_in_serve=True,
            ),
            DependencySpec("composer", use_in_build=True),
            # DependencySpec("pie", use_in_build=True),
            *self.node_provider.dependencies(),
            DependencySpec("bash", use_in_serve=True),
        ]

    def build_steps(self) -> list[str]:
        return [
            'env(COMPOSER_HOME="/tmp", COMPOSER_FUND="0")',
            'workdir(app.path)',
            # "run(\"pie install php/pdo_pgsql\")",
            'run("composer install --optimize-autoloader --no-scripts --no-interaction", inputs=["composer.json", "composer.lock", "artisan"], outputs=["."], group="install")',
            *self.node_provider.build_steps(),
        ]

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
        return [MountSpec("app"), *self.node_provider.mounts()]

    def volumes(self) -> list[VolumeSpec]:
        return []

    def env(self) -> Optional[Dict[str, str]]:
        return None

    def services(self) -> list[ServiceSpec]:
        return []
