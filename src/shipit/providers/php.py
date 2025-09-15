from __future__ import annotations

from pathlib import Path
from typing import Dict, Optional

from .base import DetectResult, DependencySpec, Provider, _exists, MountSpec


class PhpProvider:
    def __init__(self, path: Path):
        self.path = path
    @classmethod
    def name(cls) -> str:
        return "php"

    @classmethod
    def detect(cls, path: Path) -> Optional[DetectResult]:
        if _exists(path, "composer.json") and _exists(path, "public/index.php"):
            return DetectResult(cls.name(), 60)
        if _exists(path, "index.php") and not _exists(path, "composer.json"):
            return DetectResult(cls.name(), 10)
        return None

    def initialize(self) -> None:
        pass

    def serve_name(self) -> str:
        return self.path.name

    def provider_kind(self) -> str:
        return "php"

    def has_composer(self) -> bool:
        return _exists(self.path, "composer.json", "composer.lock")

    def dependencies(self) -> list[DependencySpec]:
        deps = [
            DependencySpec(
                "php",
                env_var="SHIPIT_PHP_VERSION",
                default_version="8.3",
                use_in_build=True,
                use_in_serve=True,
            ),
        ]
        if self.has_composer():
            deps.append(DependencySpec("composer", use_in_build=True))
            deps.append(DependencySpec("bash", use_in_serve=True))
        return deps

    def declarations(self) -> Optional[str]:
        return "HOME = getenv(\"HOME\")"

    def build_steps(self) -> list[str]:
        steps = [
            "workdir(app[\"build\"])",
        ]

        if self.has_composer():
            steps.append("env(HOME=HOME, COMPOSER_FUND=\"0\")")
            steps.append("run(\"composer install --optimize-autoloader --no-scripts --no-interaction\", inputs=[\"composer.json\", \"composer.lock\"], outputs=[\".\"], group=\"install\")")

        steps.append("copy(\".\", \".\", ignore=[\".git\"])")
        return steps

    def prepare_steps(self) -> Optional[list[str]]:
        return None

    def commands(self) -> Dict[str, str]:
        if _exists(self.path, "public/index.php"):
            return {"start": '"php -S localhost:8080 -t public"'}
        elif _exists(self.path, "index.php"):
            return {"start": '"php -S localhost:8080" -t .'}

    def assets(self) -> Optional[Dict[str, str]]:
        return {"php.ini": "get_asset(\"php/php.ini\")"}

    def mounts(self) -> list[MountSpec]:
        return [MountSpec("app")]

    def env(self) -> Optional[Dict[str, str]]:
        return None
