import json
from enum import Enum
from pathlib import Path
from typing import Any, Literal, Optional

from .base import DetectResult, _exists, Config
from pydantic_settings import SettingsConfigDict


class PhpFramework(Enum):
    Laravel = "laravel"
    Moodle = "moodle"
    Symfony = "symfony"
    Drupal = "drupal"


class PhpConfig(Config):
    model_config = SettingsConfigDict(extra="ignore", env_prefix="SHIPIT_")

    framework: Optional[PhpFramework] = None
    phpix: bool = False
    use_composer: bool = False
    composer_build_script: Optional[str] = None
    php_version: Optional[str] = "8.3.29"
    php_architecture: Optional[Literal["64-bit", "32-bit"]] = None
    phpix_worker_threads: Optional[int] = 4
    # Docroot subdirectory ("web", "public", "app") or None for the app root.
    public_dir: Optional[str] = None


class PhpProvider:
    def __init__(self, path: Path, config: PhpConfig):
        self.path = path
        self.config = config

    @staticmethod
    def load_composer_config(path: Path) -> dict[str, Any] | None:
        composer_path = path / "composer.json"
        if not composer_path.exists():
            return None
        try:
            composer_config = json.loads(composer_path.read_text())
        except json.JSONDecodeError:
            return None
        if not isinstance(composer_config, dict):
            return None
        return composer_config

    @staticmethod
    def composer_packages(composer_config: dict[str, Any] | None) -> set[str]:
        if not composer_config:
            return set()

        packages: set[str] = set()
        for section in ("require", "require-dev"):
            deps = composer_config.get(section)
            if isinstance(deps, dict):
                packages.update(str(name).lower() for name in deps)

        package_name = composer_config.get("name")
        if isinstance(package_name, str):
            packages.add(package_name.lower())
        return packages

    @classmethod
    def detect_framework(
        cls,
        path: Path,
        composer_config: dict[str, Any] | None = None,
    ) -> PhpFramework | None:
        composer_config = composer_config or cls.load_composer_config(path)
        composer_packages = cls.composer_packages(composer_config)

        has_moodle_layout = (
            (path / "version.php").exists()
            and (path / "lib" / "setup.php").exists()
            and (
                (path / "admin" / "cli" / "install.php").exists()
                or (path / "mod").is_dir()
                or (path / "theme").is_dir()
            )
        )
        if has_moodle_layout:
            return PhpFramework.Moodle

        drupal_packages = {
            "drupal/core",
            "drupal/core-composer-scaffold",
            "drupal/core-recommended",
            "drupal/drupal",
            "drupal/recommended-project",
        }
        has_drupal_layout = (
            (path / "core" / "lib" / "Drupal.php").exists()
            or (path / "web" / "core" / "lib" / "Drupal.php").exists()
        )
        if has_drupal_layout or composer_packages & drupal_packages:
            return PhpFramework.Drupal

        if (path / "artisan").exists() and (path / "composer.json").exists():
            return PhpFramework.Laravel

        if (path / "symfony.lock").exists() or any(
            package.startswith("symfony/") for package in composer_packages
        ):
            return PhpFramework.Symfony

        return None

    @classmethod
    def load_config(cls, path: Path, base_config: Config) -> PhpConfig:
        composer_config = cls.load_composer_config(path)
        use_composer = (
            _exists(path, "composer.json", "composer.lock")
            or (
                base_config.commands.install
                and base_config.commands.install.startswith("composer ")
            )
            or False
        )
        composer_build_script = None
        if composer_config:
            scripts = composer_config.get("scripts")
            if isinstance(scripts, dict):
                composer_build_script = (
                    "post-update-cmd" if "post-update-cmd" in scripts else None
                )
                if not composer_build_script and "post-install-cmd" in scripts:
                    composer_build_script = "post-install-cmd"
        config = PhpConfig(
            use_composer=use_composer,
            composer_build_script=composer_build_script,
            **base_config.model_dump(),
        )
        if not config.framework:
            config.framework = cls.detect_framework(path, composer_config)
        if config.framework == PhpFramework.Drupal:
            # Drupal relies on Apache-style rewrite behavior that the built-in
            # php server handles more predictably than phpix by default.
            config.phpix = False
        if config.framework == PhpFramework.Drupal and _exists(path, "web/index.php"):
            config.public_dir = "web"
        elif _exists(path, "public/index.php"):
            config.public_dir = "public"
        elif _exists(path, "app/index.php"):
            config.public_dir = "app"
        else:
            config.public_dir = None
        return config

    @classmethod
    def name(cls) -> str:
        return "php"

    @classmethod
    def detect(
        cls, path: Path, config: Config
    ) -> Optional[DetectResult]:
        framework = cls.detect_framework(path)
        if framework == PhpFramework.Drupal and (path / "web" / "index.php").exists():
            return DetectResult(cls.name(), 70)
        if framework in {
            PhpFramework.Drupal,
            PhpFramework.Moodle,
            PhpFramework.Symfony,
        } and _exists(path, "index.php", "public/index.php", "web/index.php"):
            return DetectResult(cls.name(), 65)
        if (path / "composer.json").exists() and _exists(
            path, "public/index.php", "web/index.php"
        ):
            return DetectResult(cls.name(), 60)
        if (
            _exists(path, "index.php")
            or _exists(path, "public/index.php")
            or _exists(path, "web/index.php")
            or _exists(path, "app/index.php")
        ):
            return DetectResult(cls.name(), 10)
        if config.commands.start and config.commands.start.startswith("php "):
            return DetectResult(cls.name(), 70)
        if config.commands.install and config.commands.install.startswith("composer "):
            return DetectResult(cls.name(), 30)
        return None

