import re
from dataclasses import dataclass
from pathlib import Path
from typing import Literal, Optional

from pydantic_settings import SettingsConfigDict

from .base import Config, DetectResult, _exists
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
    # Set when the project is a plugin/theme rather than a full site.
    wp_extension_kind: Optional[Literal["plugin", "theme"]] = None
    wp_extension_slug: Optional[str] = None
    wp_extension_activate_target: Optional[str] = None


class WordPressProvider(PhpProvider):
    HEADER_SCAN_BYTES = 8192
    PLUGIN_HEADER_RE = re.compile(
        r"^[ \t/*#@]*Plugin Name\s*:", re.I | re.M
    )
    THEME_HEADER_RE = re.compile(
        r"^[ \t/*#@]*Theme Name\s*:", re.I | re.M
    )
    TEXT_DOMAIN_RE = re.compile(
        r"^[ \t/*#@]*Text Domain\s*:\s*([^\s*/#@]+)", re.I | re.M
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
        extension = cls.detect_extension(path)
        if extension and not wp_config.wp_version:
            wp_config.wp_version = "latest"
        if extension:
            wp_config.wp_extension_kind = extension.kind
            wp_config.wp_extension_slug = extension.slug
            wp_config.wp_extension_activate_target = extension.activate_target
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
            slug = cls._detect_text_domain_slug(plugin_file) or cls._slugify(
                path.name
            )
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
        slug = cls._detect_text_domain_slug(style_css) or cls._slugify(
            path.name
        )
        return WordPressExtension(
            kind="theme",
            slug=slug,
            activate_target=slug,
            source_file="style.css",
        )

    @classmethod
    def _detect_text_domain_slug(cls, header_file: Path) -> Optional[str]:
        text_domain = cls._file_header_value(
            header_file,
            cls.TEXT_DOMAIN_RE,
        )
        if not text_domain:
            return None
        return cls._slugify(text_domain)

    @classmethod
    def _file_has_header(cls, path: Path, pattern: re.Pattern[str]) -> bool:
        return cls._file_header_value(path, pattern) is not None

    @classmethod
    def _file_header_value(
        cls, path: Path, pattern: re.Pattern[str]
    ) -> Optional[str]:
        if not path.is_file():
            return None
        try:
            contents = path.read_text(errors="ignore")[: cls.HEADER_SCAN_BYTES]
        except OSError:
            return None
        match = pattern.search(contents)
        if not match:
            return None
        if match.lastindex:
            return match.group(1).strip()
        return match.group(0).strip()

    @staticmethod
    def _slugify(value: str) -> str:
        slug = re.sub(r"[^a-z0-9_-]+", "-", value.lower()).strip("-_")
        return slug or "wordpress-extension"

