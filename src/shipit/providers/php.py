import json
from pathlib import Path
from typing import Dict, Optional, Literal

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


class PhpConfig(Config):
    model_config = SettingsConfigDict(extra="ignore", env_prefix="SHIPIT_")

    use_composer: bool = False
    composer_build_script: Optional[str] = None
    php_version: Optional[str] = "8.3"
    php_architecture: Optional[Literal["64-bit", "32-bit"]] = "64-bit"


class PhpProvider:
    def __init__(self, path: Path, config: PhpConfig):
        self.path = path
        self.config = config

    @classmethod
    def load_config(cls, path: Path, base_config: Config) -> PhpConfig:
        use_composer = (
            _exists(path, "composer.json", "composer.lock")
            or (
                base_config.commands.install
                and base_config.commands.install.startswith("composer ")
            )
            or False
        )
        composer_build_script = None
        if use_composer:
            composer_config = json.load(open(path / "composer.json"))
            if "scripts" in composer_config:
                assert isinstance(composer_config["scripts"], dict), "Scripts must be a dictionary"
                composer_build_script = "post-update-cmd" if "post-update-cmd" in composer_config["scripts"] else None
                if not composer_build_script and "post-install-cmd" in composer_config["scripts"]:
                    composer_build_script = "post-install-cmd"
        config = PhpConfig(use_composer=use_composer, composer_build_script=composer_build_script, **base_config.model_dump())
        return config

    @classmethod
    def name(cls) -> str:
        return "php"

    @classmethod
    def detect(
        cls, path: Path, config: Config
    ) -> Optional[DetectResult]:
        if _exists(path, "composer.json") and _exists(path, "public/index.php"):
            return DetectResult(cls.name(), 60)
        if (
            _exists(path, "index.php")
            or _exists(path, "public/index.php")
            or _exists(path, "app/index.php")
        ):
            return DetectResult(cls.name(), 10)
        if config.commands.start and config.commands.start.startswith("php "):
            return DetectResult(cls.name(), 70)
        if config.commands.install and config.commands.install.startswith("composer "):
            return DetectResult(cls.name(), 30)
        return None

    def serve_name(self) -> Optional[str]:
        return None

    def dependencies(self) -> list[DependencySpec]:
        deps = [
            DependencySpec(
                "php",
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

    def build_steps(self) -> list[str]:
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
            steps.append('env(COMPOSER_HOME="/tmp", COMPOSER_FUND="0")')
            steps.append(
                'run("composer install --optimize-autoloader --ignore-platform-reqs --no-scripts --no-interaction", inputs=["composer.json", "composer.lock"], outputs=["."], group="install")'
            )

        steps.append('copy(".", ".", ignore=[".git"])')

        # Since we don't run the scripts during the install step, we need to run them after the build step
        if self.config.use_composer and self.config.composer_build_script:
            steps.append(f'run("composer run-script {self.config.composer_build_script}", outputs=["."], group="build")')

        return steps

    def prepare_steps(self) -> Optional[list[str]]:
        return None

    def commands(self) -> Dict[str, str]:
        return self.base_commands()

    def base_commands(self) -> Dict[str, str]:
        if _exists(self.path, "public/index.php"):
            return {
                "start": '"php -S localhost:{} -t {}/public".format(PORT, app.serve_path)'
            }
        elif _exists(self.path, "app/index.php"):
            return {
                "start": '"php -S localhost:{} -t {}/app".format(PORT, app.serve_path)'
            }
        elif _exists(self.path, "index.php"):
            return {"start": '"php -S localhost:{} -t {}".format(PORT, app.serve_path)'}
        return {
            "start": '"php -S localhost:{} -t {}".format(PORT, app.serve_path)',
        }

    def mounts(self) -> list[MountSpec]:
        return [
            MountSpec("app"),
            MountSpec("assets"),
        ]

    def volumes(self) -> list[VolumeSpec]:
        return []

    def env(self) -> Optional[Dict[str, str]]:
        return {
            "PHP_INI_SCAN_DIR": '"{}".format(assets.serve_path)',
        }

    def services(self) -> list[ServiceSpec]:
        return []
