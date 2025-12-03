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
from .php import PhpMetadata, PhpProvider
from .node_static import NodeStaticMetadata, NodeStaticProvider
from pydantic_settings import SettingsConfigDict


class LaravelMetadata(PhpMetadata, NodeStaticMetadata):
    model_config = SettingsConfigDict(extra="ignore", env_prefix="SHIPIT_")


class LaravelProvider(PhpProvider):
    def __init__(self, path: Path, metadata: LaravelMetadata):
        self.path = path
        self.node_provider = NodeStaticProvider(path, metadata, only_build=True)
        self.metadata = metadata

    @classmethod
    def load_metadata(cls, path: Path, custom_commands: CustomCommands) -> LaravelMetadata:
        metadata = super().load_metadata(path, custom_commands)
        node_metadata = NodeStaticProvider.load_metadata(path, custom_commands)
        node_metadata.static_dir = None
        node_metadata.static_generator = None
        return LaravelMetadata(**metadata.model_dump(), **node_metadata.model_dump())

    @classmethod
    def name(cls) -> str:
        return "laravel"

    @classmethod
    def detect(cls, path: Path, custom_commands: CustomCommands) -> Optional[DetectResult]:
        if _exists(path, "artisan") and _exists(path, "composer.json"):
            return DetectResult(cls.name(), 95)
        return None

    def serve_name(self) -> Optional[str]:
        return None

    def dependencies(self) -> list[DependencySpec]:
        return [
            DependencySpec(
                "php",
                var_name="metadata.php_version",
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
            "env(COMPOSER_HOME=\"/tmp\", COMPOSER_FUND=\"0\")",
            "workdir(app[\"build\"])",
            # "run(\"pie install php/pdo_pgsql\")",
            "run(\"composer install --optimize-autoloader --no-scripts --no-interaction\", inputs=[\"composer.json\", \"composer.lock\", \"artisan\"], outputs=[\".\"], group=\"install\")",
            *self.node_provider.build_steps(),
        ]

    def prepare_steps(self) -> Optional[list[str]]:
        return [
            'workdir(app["serve"])',
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
