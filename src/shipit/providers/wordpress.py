import json
import re
from dataclasses import dataclass
from functools import cached_property
from pathlib import Path
from typing import Dict, Literal, Optional

from pydantic_settings import SettingsConfigDict

from .base import (
    Config,
    DetectResult,
    DependencySpec,
    MountSpec,
    ServiceSpec,
    VolumeSpec,
    _exists,
)
from .php import PhpConfig, PhpProvider


@dataclass(frozen=True)
class WordPressExtension:
    kind: Literal["plugin", "theme"]
    slug: str
    activate_target: str
    source_file: Optional[str] = None

    @property
    def content_dir(self) -> str:
        return f"{self.kind}s"


class WordPressConfig(PhpConfig):
    model_config = SettingsConfigDict(extra="ignore", env_prefix="SHIPIT_")

    wp_version: Optional[str] = None
    wp_locale: Optional[str] = None
    wp_cli_version: Optional[str] = None


class WordPressProvider(PhpProvider):
    HEADER_SCAN_BYTES = 8192
    PLUGIN_HEADER_RE = re.compile(
        r"^[ \t/*#@]*Plugin Name\s*:", re.I | re.M
    )
    THEME_HEADER_RE = re.compile(
        r"^[ \t/*#@]*Theme Name\s*:", re.I | re.M
    )

    @classmethod
    def name(cls) -> str:
        return "wordpress"

    @classmethod
    def load_config(
        cls, path: Path, config: Config
    ) -> WordPressConfig:
        php_config = super().load_config(path, config)
        wp_config = WordPressConfig(**php_config.model_dump())
        if cls.detect_extension(path) and not wp_config.wp_version:
            wp_config.wp_version = "latest"
        return wp_config

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

        wp_config = cls.load_config(path, config)
        if wp_config.wp_version:
            return DetectResult(cls.name(), 80)
        if cls.detect_extension(path):
            return DetectResult(cls.name(), 75)
        return None

    @classmethod
    def detect_extension(cls, path: Path) -> Optional[WordPressExtension]:
        theme = cls._detect_theme(path)
        if theme:
            return theme
        return cls._detect_plugin(path)

    @classmethod
    def _detect_plugin(cls, path: Path) -> Optional[WordPressExtension]:
        plugin_files = sorted(path.glob("*.php"))
        for plugin_file in plugin_files:
            if not cls._file_has_header(plugin_file, cls.PLUGIN_HEADER_RE):
                continue
            slug = cls._slugify(path.name)
            return WordPressExtension(
                kind="plugin",
                slug=slug,
                activate_target=f"{slug}/{plugin_file.name}",
                source_file=plugin_file.name,
            )
        return None

    @classmethod
    def _detect_theme(cls, path: Path) -> Optional[WordPressExtension]:
        style_css = path / "style.css"
        if not cls._file_has_header(style_css, cls.THEME_HEADER_RE):
            return None
        if not (
            (path / "index.php").is_file()
            or (path / "templates" / "index.html").is_file()
            or (path / "theme.json").is_file()
        ):
            return None
        slug = cls._slugify(path.name)
        return WordPressExtension(
            kind="theme",
            slug=slug,
            activate_target=slug,
            source_file="style.css",
        )

    @classmethod
    def _file_has_header(cls, path: Path, pattern: re.Pattern[str]) -> bool:
        if not path.is_file():
            return False
        try:
            contents = path.read_text(errors="ignore")[: cls.HEADER_SCAN_BYTES]
        except OSError:
            return False
        return bool(pattern.search(contents))

    @staticmethod
    def _slugify(value: str) -> str:
        slug = re.sub(r"[^a-z0-9_-]+", "-", value.lower()).strip("-_")
        return slug or "wordpress-extension"

    @cached_property
    def extension(self) -> Optional[WordPressExtension]:
        return self.detect_extension(self.path)

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

    def _wordpress_base_build_steps(self) -> list[str]:
        steps = [
            'copy(wp_cli_download_url, "{}/wp-cli.phar".format(assets.path))',
            'copy("wordpress/install.sh", "{}/setup-wp.sh".format(assets.path), base="assets")',
        ]
        if self.config.wp_version:
            version_args = [f"--version={self.config.wp_version}"]
            if self.config.wp_locale:
                version_args.append(f"--locale={self.config.wp_locale}")
            version_flags = " ".join(version_args)
            steps.append(
                'run("php -d memory_limit=512M {}/wp-cli.phar core '
                "download --allow-root --path={} "
                f'{version_flags}".format(assets.path, app.path))'
            )
        if self.config.phpix:
            # We create the start script that creates the .htaccess symlink
            # since phpix now supports .htaccess files.
            steps.append(
                'copy("wordpress/start.php", "{}/start-wp.php".format(assets.path), base="assets")'
            )
        if self.extension or not _exists(self.path, "wp-config.php"):
            steps.append(
                'copy("wordpress/wp-config.php", "{}/wp-config.php".format(app.path), base="assets")'
            )
        if self.extension or not _exists(self.path, ".htaccess"):
            steps.append(
                'copy("wordpress/.htaccess", "{}/.htaccess".format(app.path), base="assets")'
            )
        return steps

    def _wp_content_seed_steps(self) -> list[str]:
        steps = []
        if self.config.wp_version:
            steps.append(
                'run("cp -R {}/wp-content/* {}".format(app.path, wpcontent_base.path))'
            )
        if _exists(self.path, "wp-content"):
            steps.append('copy("wp-content", "{}".format(wpcontent_base.path))')
        return steps

    def _php_asset_steps(self) -> list[str]:
        steps = [
            'workdir(app.path)',
        ]
        if _exists(self.path, "php.ini"):
            steps.append('copy("php.ini", "{}/php.ini".format(assets.path))')
        else:
            steps.append(
                'copy("php/php.ini", "{}/php.ini".format(assets.path), base="assets")'
            )
        return steps

    def _extension_build_steps(self, extension: WordPressExtension) -> list[str]:
        target = (
            f'"{{}}/{extension.content_dir}/{extension.slug}".format('
            "wpcontent_base.path)"
        )
        ignore = [".git", ".source"]
        if self.config.use_composer:
            ignore.append("vendor")

        steps = [
            *self._wordpress_base_build_steps(),
            *self._php_asset_steps(),
            *self._wp_content_seed_steps(),
            f'copy(".", {target}, ignore={json.dumps(ignore)})',
        ]

        if self.config.use_composer:
            steps.extend(
                [
                    f"workdir({target})",
                    (
                        'env(COMPOSER_HOME="/tmp", COMPOSER_FUND="0", '
                        'COMPOSER_ALLOW_SUPERUSER="1")'
                    ),
                    (
                        'run("composer install --optimize-autoloader '
                        '--ignore-platform-reqs --no-scripts '
                        '--no-interaction", group="install")'
                    ),
                ]
            )
            if self.config.composer_build_script:
                steps.append(
                    f'run("composer run-script {self.config.composer_build_script}", '
                    'outputs=["."], group="build")'
                )

        return steps

    def build_steps(self) -> list[str]:
        if self.extension:
            return self._extension_build_steps(self.extension)

        return (
            self._wordpress_base_build_steps()
            + super().build_steps_with_options(
                extra_ignore=["wp-content"],
                after_install=None,
                after_build=None,
            )
            + self._wp_content_seed_steps()
        )

    def prepare_steps(self) -> Optional[list[str]]:
        return super().prepare_steps()

    def commands(self) -> Dict[str, str]:
        commands = super().commands()
        if self.config.phpix:
            if "start" in commands:
                commands["start"] = (
                    '"phpix --startup-script={}/start-wp.php -S localhost:{} '
                    '-t {}".format(assets.serve_path, PORT, app.serve_path)'
                )
        return {
            "wp": '"php {}/wp-cli.phar --allow-root --path={}".format(assets.serve_path, app.serve_path)',
            "after_deploy": '"bash {}/setup-wp.sh".format(assets.serve_path)',
            **commands,
        }

    def mounts(self) -> list[MountSpec]:
        return super().mounts() + [MountSpec("wpcontent_base")]

    def volumes(self) -> list[VolumeSpec]:
        return [
            VolumeSpec(
                name="wp-content",
                serve_path='"{}/wp-content/".format(app.serve_path)',
                var_name="wp_content",
            )
        ]

    def env(self) -> Optional[Dict[str, str]]:
        env = {
            "PAGER": '"cat"',
            "WPCONTENT_BASE_PATH": '"{}".format(wpcontent_base.serve_path)',
            **(super().env() or {}),
        }
        if self.config.wp_locale:
            env["WP_LOCALE"] = f'"{self.config.wp_locale}"'
        if self.extension and self.extension.kind == "plugin":
            env["WP_PLUGINS_ACTIVATE"] = f'"{self.extension.activate_target}"'
        if self.extension and self.extension.kind == "theme":
            env["WP_DEFAULT_THEME"] = f'"{self.extension.activate_target}"'
        return env

    def services(self) -> list[ServiceSpec]:
        return [ServiceSpec(name="database", provider="mysql")]
