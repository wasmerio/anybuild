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
    Config,
)
from .php import PhpConfig, PhpProvider
from pydantic_settings import SettingsConfigDict


class WordPressConfig(PhpConfig):
    model_config = SettingsConfigDict(extra="ignore", env_prefix="SHIPIT_")

    wp_cli_version: Optional[str] = None


class WordPressProvider(PhpProvider):
    @classmethod
    def name(cls) -> str:
        return "wordpress"

    @classmethod
    def load_config(
        cls, path: Path, config: Config
    ) -> WordPressConfig:
        php_config = super().load_config(path, config)
        return WordPressConfig(**php_config.model_dump())

    @classmethod
    def detect(
        cls, path: Path, config: Config
    ) -> Optional[DetectResult]:
        if (
            _exists(path, "wp-content")
            and _exists(path, "index.php")
            and _exists(path, "wp-load.php")
        ):
            return DetectResult(cls.name(), 80)
        return None

    def serve_name(self) -> Optional[str]:
        return None

    def dependencies(self) -> list[DependencySpec]:
        return [
            *super().dependencies(),
            DependencySpec("bash", use_in_build=False, use_in_serve=True),
        ]

    def declarations(self) -> Optional[str]:
        return (super().declarations() or "") + (
            "wp_cli_version = config.wp_cli_version\n"
            'wp_cli_download_url = f"https://github.com/wp-cli/wp-cli/releases/download/v{wp_cli_version}/wp-cli-{wp_cli_version}.phar" if wp_cli_version else "https://raw.githubusercontent.com/wp-cli/builds/gh-pages/phar/wp-cli.phar"\n'
        )

    def build_steps(self) -> list[str]:
        steps = [
            'copy(wp_cli_download_url, "{}/wp-cli.phar".format(assets.path))',
            'copy("wordpress/install.sh", "{}/setup-wp.sh".format(assets.path), base="assets")',
        ]
        if not _exists(self.path, "wp-config.php"):
            steps.append(
                'copy("wordpress/wp-config.php", "{}/wp-config.php".format(app.path), base="assets")'
            )
        return steps + super().build_steps()

    def prepare_steps(self) -> Optional[list[str]]:
        return super().prepare_steps()

    def commands(self) -> Dict[str, str]:
        commands = super().commands()
        return {
            # "wp": '"php {}/wp-cli.phar --allow-root --path={}".format(assets.serve_path, app.serve_path)',
            "after_deploy": '"bash {}/setup-wp.sh".format(assets.serve_path)',
            **commands,
        }

    def mounts(self) -> list[MountSpec]:
        return super().mounts()

    def volumes(self) -> list[VolumeSpec]:
        return [
            VolumeSpec(
                name="wp-content",
                serve_path='"{}/wp-content/".format(app.serve_path)',
                var_name="wp_content",
            )
        ]

    def env(self) -> Optional[Dict[str, str]]:
        return {
            "PAGER": '"cat"',
            **(super().env() or {}),
        }

    def services(self) -> list[ServiceSpec]:
        return [ServiceSpec(name="database", provider="mysql")]
