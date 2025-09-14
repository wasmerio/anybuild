from __future__ import annotations

from pathlib import Path
from typing import Dict, Optional

from .base import DetectResult, DependencySpec, Provider, _exists, MountSpec


class PhpProvider:
    def name(self) -> str:
        return "php"

    def detect(self, path: Path) -> Optional[DetectResult]:
        if _exists(path, "composer.json") and _exists(path, "public/index.php"):
            return DetectResult(self.name(), 60)
        if _exists(path, "index.php") and not _exists(path, "composer.json"):
            return DetectResult(self.name(), 10)
        return None

    def initialize(self, path: Path) -> None:
        pass

    def serve_name(self, path: Path) -> str:
        return path.name

    def provider_kind(self, path: Path) -> str:
        return "php"

    def has_composer(self, path: Path) -> bool:
        return _exists(path, "composer.json", "composer.lock")

    def dependencies(self, path: Path) -> list[DependencySpec]:
        deps = [
            DependencySpec(
                "php",
                env_var="SHIPIT_PHP_VERSION",
                default_version="8.3",
                use_in_build=True,
                use_in_serve=True,
            ),
        ]
        if self.has_composer(path):
            deps.append(DependencySpec("composer", use_in_build=True))
            deps.append(DependencySpec("bash", use_in_serve=True))
        return deps

    def declarations(self, path: Path) -> Optional[str]:
        return "HOME = getenv(\"HOME\")"

    def build_steps(self, path: Path) -> list[str]:
        steps = [
            "workdir(app[\"build\"])",
        ]

        if self.has_composer(path):
            steps.append("env(HOME=HOME, COMPOSER_FUND=\"0\")")
            steps.append("run(\"composer install --optimize-autoloader --no-scripts --no-interaction\", inputs=[\"composer.json\", \"composer.lock\"], outputs=[\".\"], group=\"install\")")

        steps.append("copy(\".\", \".\", ignore=[\".git\"])")
        return steps

    def prepare_steps(self, path: Path) -> Optional[list[str]]:
        return None

    def commands(self, path: Path) -> Dict[str, str]:
        if _exists(path, "public/index.php"):
            return {"start": '"php -S localhost:8080 -t public"'}
        elif _exists(path, "index.php"):
            return {"start": '"php -S localhost:8080" -t .'}

    def assets(self, path: Path) -> Optional[Dict[str, str]]:
        return {"php.ini": "get_asset(\"php/php.ini\")"}

    def mounts(self, path: Path) -> list[MountSpec]:
        return [MountSpec("app")]

    def env(self, path: Path) -> Optional[Dict[str, str]]:
        return None
