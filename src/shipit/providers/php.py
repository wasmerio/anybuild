import json
from enum import Enum
from pathlib import Path
from typing import Any, Dict, Literal, Optional

from .base import (
    DetectResult,
    DependencySpec,
    Provider,
    _exists,
    MountSpec,
    ServiceSpec,
    VolumeSpec,
    Config,
)
from pydantic_settings import SettingsConfigDict


class PhpFramework(Enum):
    Laravel = "laravel"
    Moodle = "moodle"
    Symfony = "symfony"
    Drupal = "drupal"


class PhpConfig(Config):
    model_config = SettingsConfigDict(extra="ignore", env_prefix="SHIPIT_")

    framework: Optional[PhpFramework] = None
    phpix: bool = True
    use_composer: bool = False
    composer_build_script: Optional[str] = None
    php_version: Optional[str] = "8.3.29"
    php_architecture: Optional[Literal["64-bit", "32-bit"]] = None
    phpix_worker_threads: Optional[int] = 4


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

    def dependencies(self) -> list[DependencySpec]:
        deps = [
            DependencySpec(
                "php" if not self.config.phpix else "phpix",
                var_name="config.php_version",
                architecture_var_name="config.php_architecture",
                use_in_build=True,
                use_in_serve=True,
            ),
        ]
        if self.config.use_composer:
            deps.append(DependencySpec("composer", use_in_build=True))
            deps.append(DependencySpec("bash", use_in_serve=True))
        return deps

    def declarations(self) -> Optional[str]:
        return None

    def build_steps_with_options(self, extra_ignore: Optional[list[str]] = None, after_install: Optional[list[str]] = None, after_build: Optional[list] = None) -> list[str]:
        steps = [
            'workdir(app.path)',
        ]
        if _exists(self.path, "php.ini"):
            steps.append('copy("php.ini", "{}/php.ini".format(assets.path))')
        else:
            steps.append(
                'copy("php/php.ini", "{}/php.ini".format(assets.path), base="assets")'
            )

        if self.config.use_composer:
            steps.append('env(COMPOSER_HOME="/tmp", COMPOSER_FUND="0", COMPOSER_ALLOW_SUPERUSER="1")')
            steps.append(
                'run("composer install --optimize-autoloader --ignore-platform-reqs --no-scripts --no-interaction", inputs=["composer.json", "composer.lock"], outputs=["."], group="install")'
            )

        if after_install:
            assert isinstance(after_install, list), "after_install must be a list if provided"
            steps = steps + after_install

        dirs_to_ignore = [".git"]
        if extra_ignore:
            assert isinstance(extra_ignore, list), "extra_ignore must be a list if provided"
            dirs_to_ignore += extra_ignore

        if self.config.use_composer:
            dirs_to_ignore.append("vendor")
        if self.config.framework == PhpFramework.Symfony:
            dirs_to_ignore.append("var")

        steps.append('copy(".", ignore={})'.format(json.dumps(dirs_to_ignore)))

        # Since we don't run the scripts during the install step, we need to run them after the build step
        if self.config.use_composer and self.config.composer_build_script:
            steps.append(f'run("composer run-script {self.config.composer_build_script}", outputs=["."], group="build")')

        if after_build:
            assert isinstance(after_build, list), "after_build must be a list if provided"
            steps = steps + after_build

        return steps

    def build_steps(self) -> list[str]:
        return self.build_steps_with_options(
            extra_ignore=None,
            after_install=None,
            after_build=None
        )

    def prepare_steps(self) -> Optional[list[str]]:
        return None

    def commands(self) -> Dict[str, str]:
        return self.base_commands()

    def base_commands(self) -> Dict[str, str]:
        php_script = "phpix" if self.config.phpix else "php"

        if (
            self.config.framework == PhpFramework.Drupal
            and _exists(self.path, "web/index.php")
        ):
            return {
                "start": (
                    f'"{php_script} -S localhost:{{}} -t '
                    '{}/web".format(PORT, app.serve_path)'
                )
            }
        if _exists(self.path, "public/index.php"):
            return {
                "start": f'"{php_script} -S localhost:{{}} -t {{}}/public".format(PORT, app.serve_path)'
            }
        elif _exists(self.path, "app/index.php"):
            return {
                "start": f'"{php_script} -S localhost:{{}} -t {{}}/app".format(PORT, app.serve_path)'
            }
        elif _exists(self.path, "index.php"):
            return {"start": f'"{php_script} -S localhost:{{}} -t {{}}".format(PORT, app.serve_path)'}
        return {
            "start": f'"{php_script} -S localhost:{{}} -t {{}}".format(PORT, app.serve_path)',
        }

    def mounts(self) -> list[MountSpec]:
        return [
            MountSpec("app"),
            MountSpec("assets"),
        ]

    def volumes(self) -> list[VolumeSpec]:
        return []

    def env(self) -> Optional[Dict[str, str]]:
        env = {
            "PHP_INI_SCAN_DIR": '"{}".format(assets.serve_path)',
        }
        if self.config.phpix and self.config.phpix_worker_threads:
            env["PHPIX_PHP_THREADS"] = f'"{self.config.phpix_worker_threads}"'
        return env

    def services(self) -> list[ServiceSpec]:
        return []
