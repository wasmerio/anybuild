import json
import re
import shlex
from dataclasses import dataclass
from functools import cached_property
from pathlib import Path
from typing import Dict, Optional

from tomlkit import aot, document, table
import yaml
from pydantic_settings import SettingsConfigDict

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


class StaticFileConfig(Config):
    model_config = SettingsConfigDict(extra="ignore", env_prefix="SHIPIT_")

    convert_redirects: bool = True
    sws_version: Optional[str] = "2.38.0"
    static_dir: Optional[str] = None
    # Rendered sws.toml redirects (from a _redirects file), computed at load
    # time so the Starlark provider stays filesystem-free.
    redirects_config: Optional[str] = None


@dataclass(frozen=True)
class RedirectRule:
    source: str
    destination: str
    kind: int


class StaticFileProvider:
    REDIRECTS_CONFIG_FILE = "sws.toml"
    REDIRECTS_CONFIG_MOUNT = "static_config"
    REDIRECTS_SOURCE = "_redirects"
    REDIRECT_STATUS_CODES = {301, 302}
    _PARAM_PATTERN = re.compile(r":([A-Za-z][A-Za-z0-9_]*)")
    _SOURCE_TOKEN_PATTERN = re.compile(r":([A-Za-z][A-Za-z0-9_]*)|\*")

    config: Optional[dict] = None
    path: Path

    def __init__(self, path: Path, config: StaticFileConfig):
        self.path = path
        self.config = config

    @classmethod
    def load_config(
        cls, path: Path, base_config: Config
    ) -> StaticFileConfig:
        config = cls._load_static_config(path, base_config)
        config.redirects_config = compute_redirects_config(
            path, config.static_dir, config.convert_redirects
        )
        return config

    @classmethod
    def _load_static_config(
        cls, path: Path, base_config: Config
    ) -> StaticFileConfig:
        if (path / "Staticfile").exists():
            config = None
            try:
                config = yaml.safe_load((path / "Staticfile").read_text())
            except yaml.YAMLError as e:
                print(f"Error loading Staticfile: {e}")
                pass

            if config:
                return StaticFileConfig(
                    **base_config.model_dump(),
                    static_dir=config.get("root"),
                )
        if _exists(path, "public/index.html") or _exists(path, "public/index.htm"):
            return StaticFileConfig(static_dir="public", **base_config.model_dump())

        return StaticFileConfig(**base_config.model_dump())

    @classmethod
    def name(cls) -> str:
        return "staticfile"

    @classmethod
    def detect(
        cls, path: Path, config: Config
    ) -> Optional[DetectResult]:
        is_python_php_js_project = _exists(
            path, "package.json", "pyproject.toml", "composer.json"
        )
        if _exists(path, "Staticfile"):
            return DetectResult(cls.name(), 50)
        if not is_python_php_js_project:
            if _exists(
                path, "index.html", "index.htm", "public/index.htm", "public/index.html"
            ):
                return DetectResult(cls.name(), 10)
            return DetectResult(cls.name(), 10)
        if config.commands.start and config.commands.start.startswith(
            "static-web-server "
        ):
            return DetectResult(cls.name(), 70)
        return None

    def dependencies(self) -> list[DependencySpec]:
        return [
            DependencySpec(
                "static-web-server",
                var_name="config.sws_version",
                use_in_serve=True,
            )
        ]

    def build_steps_redirects(self) -> list[str]:
        redirects_config = self.redirects_config
        if not redirects_config:
            return []
        return [
            'write("{}/%s".format(static_config.path), %s)'
            % (
                self.REDIRECTS_CONFIG_FILE,
                json.dumps(redirects_config),
            )
        ]

    def build_steps(self) -> list[str]:
        source = json.dumps(self.config.static_dir or ".")
        if self.config.app_subdir:
            if self.config.static_dir:
                source = f'"{{}}/{self.config.static_dir}".format(app_subdir)'
            else:
                source = "app_subdir"
        return [
            'workdir(static_app.path)',
            f'copy({source}, ".", ignore=[".git"])',
        ] + self.build_steps_redirects()

    def prepare_steps(self) -> Optional[list[str]]:
        return None

    def declarations(self) -> Optional[str]:
        return None

    def commands(self) -> Dict[str, str]:
        if self.redirects_config:
            return {
                "start": '"static-web-server --root={} --log-level=info --config-file={}/%s --port={}".format(static_app.serve_path, static_config.serve_path, PORT)'
                % self.REDIRECTS_CONFIG_FILE
            }
        return {
            "start": '"static-web-server --root={} --log-level=info --port={}".format(static_app.serve_path, PORT)'
        }

    def mounts(self) -> list[MountSpec]:
        mounts = [MountSpec("static_app")]
        if self.redirects_config:
            mounts.append(MountSpec(self.REDIRECTS_CONFIG_MOUNT))
        return mounts

    def volumes(self) -> list[VolumeSpec]:
        return []

    def env(self) -> Optional[Dict[str, str]]:
        return None

    def services(self) -> list[ServiceSpec]:
        return []

    @cached_property
    def redirects_config(self) -> Optional[str]:
        return compute_redirects_config(
            self.path, self.config.static_dir, self.config.convert_redirects
        )

    @classmethod
    def _load_redirect_rules(cls, redirects_path: Path) -> list[RedirectRule]:
        rules: list[RedirectRule] = []
        for line_number, raw_line in enumerate(
            redirects_path.read_text().splitlines(), start=1
        ):
            line = raw_line.strip()
            if not line or line.startswith("#"):
                continue

            try:
                parts = shlex.split(line)
            except ValueError as exc:
                raise ValueError(
                    f"{redirects_path}:{line_number}: invalid _redirects rule"
                ) from exc

            if len(parts) < 2:
                raise ValueError(
                    f"{redirects_path}:{line_number}: expected source and "
                    "destination"
                )

            source, destination, *rest = parts
            kind = 301
            if rest and rest[0].isdigit():
                kind = int(rest[0])
                rest = rest[1:]

            if kind not in cls.REDIRECT_STATUS_CODES:
                raise ValueError(
                    f"{redirects_path}:{line_number}: redirect status {kind} "
                    "is not supported by static-web-server"
                )

            if rest:
                raise ValueError(
                    f"{redirects_path}:{line_number}: conditions and forced "
                    "redirects are not supported"
                )

            sws_source, replacements = cls._translate_source(
                redirects_path, line_number, source
            )
            sws_destination = cls._translate_destination(
                redirects_path, line_number, destination, replacements
            )
            rules.append(
                RedirectRule(
                    source=sws_source,
                    destination=sws_destination,
                    kind=kind,
                )
            )

        return rules

    @classmethod
    def _translate_source(
        cls, redirects_path: Path, line_number: int, source: str
    ) -> tuple[str, dict[str, int]]:
        if "://" in source:
            raise ValueError(
                f"{redirects_path}:{line_number}: redirect sources must be "
                "local paths"
            )
        if "?" in source:
            raise ValueError(
                f"{redirects_path}:{line_number}: query matching is not "
                "supported"
            )
        if not source.startswith("/"):
            raise ValueError(
                f"{redirects_path}:{line_number}: redirect sources must start "
                "with '/'"
            )

        translated_parts: list[str] = []
        replacements: dict[str, int] = {}
        last_index = 0
        next_index = 1

        for match in cls._SOURCE_TOKEN_PATTERN.finditer(source):
            translated_parts.append(source[last_index : match.start()])
            param_name = match.group(1)
            if param_name is not None:
                if param_name in replacements:
                    raise ValueError(
                        f"{redirects_path}:{line_number}: duplicate source "
                        f"parameter :{param_name}"
                    )
                replacements[param_name] = next_index
                translated_parts.append("{*}")
            else:
                if "splat" in replacements:
                    raise ValueError(
                        f"{redirects_path}:{line_number}: only one splat "
                        "segment is supported"
                    )
                replacements["splat"] = next_index
                translated_parts.append("{**}")
            next_index += 1
            last_index = match.end()

        translated_parts.append(source[last_index:])
        return "".join(translated_parts), replacements

    @classmethod
    def _translate_destination(
        cls,
        redirects_path: Path,
        line_number: int,
        destination: str,
        replacements: dict[str, int],
    ) -> str:
        if "*" in destination:
            raise ValueError(
                f"{redirects_path}:{line_number}: destination splats must use "
                ":splat"
            )

        def replace_param(match: re.Match[str]) -> str:
            param_name = match.group(1)
            if param_name not in replacements:
                raise ValueError(
                    f"{redirects_path}:{line_number}: destination references "
                    f"unknown parameter :{param_name}"
                )
            return f"${replacements[param_name]}"

        return cls._PARAM_PATTERN.sub(replace_param, destination)


def compute_redirects_config(
    path: Path, static_dir: Optional[str], convert_redirects: bool = True
) -> Optional[str]:
    """Render a _redirects file into sws.toml redirect rules (or None)."""
    if not convert_redirects:
        return None

    redirects_path = path / StaticFileProvider.REDIRECTS_SOURCE
    if static_dir:
        static_dir_redirects = path / static_dir / StaticFileProvider.REDIRECTS_SOURCE
        if static_dir_redirects.is_file():
            redirects_path = static_dir_redirects
    if not redirects_path.is_file():
        return None

    rules = StaticFileProvider._load_redirect_rules(redirects_path)
    if not rules:
        return None

    doc = document()
    advanced = table()
    redirects = aot()
    for rule in rules:
        entry = table()
        entry.add("source", rule.source)
        entry.add("destination", rule.destination)
        entry.add("kind", rule.kind)
        redirects.append(entry)

    advanced.add("redirects", redirects)
    doc.add("advanced", advanced)
    return doc.as_string()
