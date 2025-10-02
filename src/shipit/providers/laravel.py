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


class LaravelProvider:
    def __init__(self, path: Path, custom_commands: CustomCommands):
        self.path = path
        self.custom_commands = custom_commands

    @classmethod
    def name(cls) -> str:
        return "laravel"

    @classmethod
    def detect(cls, path: Path, custom_commands: CustomCommands) -> Optional[DetectResult]:
        if _exists(path, "artisan") and _exists(path, "composer.json"):
            return DetectResult(cls.name(), 95)
        return None

    def initialize(self) -> None:
        pass

    def serve_name(self) -> str:
        return self.path.name

    def provider_kind(self) -> str:
        return "php"

    def dependencies(self) -> list[DependencySpec]:
        return [
            DependencySpec(
                "php",
                env_var="SHIPIT_PHP_VERSION",
                default_version="8.3",
                use_in_build=True,
                use_in_serve=True,
            ),
            DependencySpec("composer", use_in_build=True),
            DependencySpec("pie", use_in_build=True),
            DependencySpec("pnpm", use_in_build=True),
            DependencySpec("bash", use_in_serve=True),
        ]

    def declarations(self) -> Optional[str]:
        return "HOME = getenv(\"HOME\")"

    def build_steps(self) -> list[str]:
        return [
            "env(HOME=HOME, COMPOSER_FUND=\"0\")",
            "workdir(app[\"build\"])",
            "run(\"pie install php/pdo_pgsql\")",
            "run(\"composer install --optimize-autoloader --no-scripts --no-interaction\", inputs=[\"composer.json\", \"composer.lock\", \"artisan\"], outputs=[\".\"], group=\"install\")",
            "run(\"pnpm install\", inputs=[\"package.json\", \"package-lock.json\"], outputs=[\".\"], group=\"install\")",
            "copy(\".\", \".\", ignore=[\".git\"])",
            "run(\"pnpm run build\", outputs=[\".\"], group=\"build\")",
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
            "start": '"php -S localhost:8080 -t public"',
            "after_deploy": '"php artisan migrate"',
        }

    def mounts(self) -> list[MountSpec]:
        return [MountSpec("app")]

    def volumes(self) -> list[VolumeSpec]:
        return []

    def env(self) -> Optional[Dict[str, str]]:
        return None
    
    def services(self) -> list[ServiceSpec]:
        return []
