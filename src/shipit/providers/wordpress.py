from shipit.providers.php import PhpProvider
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
)
from .php import PhpProvider


class WordPressProvider(PhpProvider):
    def __init__(self, path: Path):
        self.path = path

    @classmethod
    def name(cls) -> str:
        return "wordpress"

    @classmethod
    def detect(cls, path: Path) -> Optional[DetectResult]:
        if _exists(path, "wp-content") and _exists(path, "index.php" and _exists(path, "wp-load.php")):
            return DetectResult(cls.name(), 80)
        return None

    def initialize(self) -> None:
        pass

    def serve_name(self) -> str:
        return self.path.name

    def provider_kind(self) -> str:
        return "php"

    def dependencies(self) -> list[DependencySpec]:
        return super().dependencies()

    def declarations(self) -> Optional[str]:
        return super().declarations()

    def build_steps(self) -> list[str]:
        return 
        [
            'copy("wordpress/install.sh", "{}/wordpress-install.sh".format(assets["build"]), base="assets")',
            *super().build_steps(),
        ]

    def prepare_steps(self) -> Optional[list[str]]:
        return super().prepare_steps()

    def commands(self) -> Dict[str, str]:
        return {
            "start": '"php -S localhost:8080 -t public"',
            "wp": '"php {}/wp-cli.phar --allow-root --path={}".format(assets[\"serve\"], app[\"serve\"])',
            "after_deploy": '"bash {}/wordpress-install.sh".format(assets["serve"])',
        }

    def mounts(self) -> list[MountSpec]:
        return super().mounts()

    def volumes(self) -> list[VolumeSpec]:
        return [VolumeSpec(name="wp-content", serve_path="\"{}/wp-content/\".format(app[\"serve\"])")]

    def env(self) -> Optional[Dict[str, str]]:
        return None
    
    def services(self) -> list[ServiceSpec]:
        return [ServiceSpec(name="database", provider="mysql")]
