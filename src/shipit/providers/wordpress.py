from __future__ import annotations
from shipit.providers.php import PhpProvider

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
from .php import PhpProvider


class WordPressProvider(PhpProvider):
    def __init__(self, path: Path, custom_commands: CustomCommands):
        super().__init__(path, custom_commands)

    @classmethod
    def name(cls) -> str:
        return "wordpress"

    @classmethod
    def detect(
        cls, path: Path, custom_commands: CustomCommands
    ) -> Optional[DetectResult]:
        if (
            _exists(path, "wp-content")
            and _exists(path, "index.php")
            and _exists(path, "wp-load.php")
        ):
            return DetectResult(cls.name(), 80)
        return None

    def initialize(self) -> None:
        pass

    def serve_name(self) -> str:
        return self.path.name

    def provider_kind(self) -> str:
        return "php"

    def platform(self) -> Optional[str]:
        return "wordpress"

    def dependencies(self) -> list[DependencySpec]:
        return [
            *super().dependencies(),
            DependencySpec("bash", use_in_build=False, use_in_serve=True),
        ]

    def declarations(self) -> Optional[str]:
        return (super().declarations() or "") + (
            'wp_cli_version = getenv("SHIPIT_WPCLI_VERSION")\n'
            "if wp_cli_version:\n"
            '    wp_cli_download_url = f"https://github.com/wp-cli/wp-cli/releases/download/v{wp_cli_version}/wp-cli-{wp_cli_version}.phar"\n'
            "else:\n"
            '    wp_cli_download_url = "https://raw.githubusercontent.com/wp-cli/builds/gh-pages/phar/wp-cli.phar"\n'
        )

    def build_steps(self) -> list[str]:
        steps = [
            'copy(wp_cli_download_url, "{}/wp-cli.phar".format(assets["build"]))',
            'copy("wordpress/install.sh", "{}/setup-wp.sh".format(assets["build"]), base="assets")',
        ]
        if not _exists(self.path, "wp-config.php"):
            steps.append(
                'copy("wordpress/wp-config.php", "{}/wp-config.php".format(app["build"]), base="assets")'
            )
        return steps + super().build_steps()

    def prepare_steps(self) -> Optional[list[str]]:
        return super().prepare_steps()

    def commands(self) -> Dict[str, str]:
        commands = super().commands()
        return {
            # "wp": '"php {}/wp-cli.phar --allow-root --path={}".format(assets["serve"], app["serve"])',
            "after_deploy": '"bash {}/setup-wp.sh".format(assets["serve"])',
            **commands,
        }

    def mounts(self) -> list[MountSpec]:
        return super().mounts()

    def volumes(self) -> list[VolumeSpec]:
        return [
            VolumeSpec(
                name="wp-content",
                serve_path='"{}/wp-content/".format(app["serve"])',
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
