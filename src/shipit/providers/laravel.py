from __future__ import annotations

from pathlib import Path
from typing import Dict, Optional

from .base import DetectResult, DependencySpec, Provider, _exists


class LaravelProvider:
    def name(self) -> str:
        return "laravel"

    def detect(self, path: Path) -> Optional[DetectResult]:
        if _exists(path, "artisan") and _exists(path, "composer.json"):
            return DetectResult(self.name(), 95)
        return None

    def initialize(self, path: Path) -> None:
        pass

    def serve_name(self, path: Path) -> str:
        return "php-laravel"

    def provider_kind(self, path: Path) -> str:
        return "php"

    def build_dependencies(self, path: Path) -> list[DependencySpec]:
        return [
            DependencySpec("php", env_var="SHIPIT_PHP_VERSION", default_version="8.3"),
            DependencySpec("composer"),
            DependencySpec("pie"),
            DependencySpec("pnpm"),
        ]

    def serve_dependencies(self, path: Path) -> list[DependencySpec]:
        return [DependencySpec("php"), DependencySpec("bash")]

    def build_steps(self, path: Path) -> list[str]:
        return [
            "HOME = getenv(\"HOME\")",
            "use(php, composer, pie, pnpm)",
            "env(HOME=HOME, COMPOSER_FUND=\"0\")",
            "run(\"pie install php/pdo_pgsql\")",
            "run(\"composer install --optimize-autoloader --no-scripts --no-interaction\", inputs=[\"composer.json\", \"composer.lock\", \"artisan\"], outputs=[\".\"], group=\"install\")",
            "run(\"pnpm install\", inputs=[\"package.json\", \"package-lock.json\"], outputs=[\".\"], group=\"install\")",
            "copy(\".\", \".\", ignore=[\".git\"])",
            "run(\"pnpm run build\", outputs=[\".\"], group=\"build\")",
        ]

    def prepare_script(self, path: Path) -> Optional[str]:
        return (
            "mkdir -p storage/framework/{sessions,views,cache,testing} storage/logs bootstrap/cache\n"
            "php artisan config:cache\n"
            "php artisan event:cache\n"
            "php artisan route:cache\n"
            "php artisan view:cache\n"
        )

    def commands(self, path: Path) -> Dict[str, str]:
        return {
            "start": '"php -S localhost:8080 -t public"',
            "after_deploy": '"php artisan migrate"',
        }

    def assets(self, path: Path) -> Optional[Dict[str, str]]:
        return None

    def mounts(self, path: Path) -> Optional[Dict[str, str]]:
        return None

