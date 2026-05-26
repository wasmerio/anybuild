import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Literal, Optional, Protocol

import toml
import yaml

from shipit.providers.base import Config


PlatformName = Literal["vercel", "railway", "render", "procfile"]


@dataclass
class PlatformEntry:
    name: str
    type: str = "web"
    root: str = "."
    runtime: Optional[str] = None
    framework: Optional[str] = None
    entrypoint: Optional[str] = None
    install_command: Optional[str] = None
    build_command: Optional[str] = None
    start_command: Optional[str] = None
    pre_deploy_command: Optional[str] = None
    schedule: Optional[str] = None
    unsupported_reason: Optional[str] = None
    warnings: list[str] = field(default_factory=list)

    def is_runnable_web(self) -> bool:
        return self.type in {"web", "static"} and not self.unsupported_reason


@dataclass
class PlatformConfig:
    platform: PlatformName
    path: Path
    entries: list[PlatformEntry]
    selected_entry: Optional[PlatformEntry] = None
    warnings: list[str] = field(default_factory=list)

    def apply_to(self, config: Config) -> Config:
        entry = self.selected_entry
        if not entry:
            return config
        if entry.install_command:
            config.commands.install = entry.install_command
        if entry.build_command:
            config.commands.build = entry.build_command
        if entry.start_command:
            config.commands.start = entry.start_command
        if entry.pre_deploy_command:
            config.commands.after_deploy = entry.pre_deploy_command
        return config


class PlatformDetector(Protocol):
    name: PlatformName

    @classmethod
    def detect_all(cls, path: Path) -> list[PlatformConfig]: ...


def apply_platform_config(
    config: Config,
    platform_config: Optional[PlatformConfig],
) -> Config:
    if platform_config:
        platform_config.apply_to(config)
    return config


def load_json(path: Path) -> Optional[dict[str, Any]]:
    try:
        data = json.loads(path.read_text())
    except Exception:
        return None
    return data if isinstance(data, dict) else None


def load_toml(path: Path) -> Optional[dict[str, Any]]:
    try:
        data = toml.loads(path.read_text())
    except Exception:
        return None
    return data if isinstance(data, dict) else None


def load_yaml(path: Path) -> Optional[dict[str, Any]]:
    try:
        data = yaml.safe_load(path.read_text())
    except Exception:
        return None
    return data if isinstance(data, dict) else None


def string(value: Any) -> Optional[str]:
    return value if isinstance(value, str) and value else None


def list_commands(value: Any) -> Optional[str]:
    if isinstance(value, str):
        return value
    if isinstance(value, list) and all(isinstance(item, str) for item in value):
        return " && ".join(value)
    return None


def runtime_start_command(
    runtime: Optional[str],
    entrypoint: Optional[str],
) -> Optional[str]:
    if not entrypoint:
        return None
    suffix = Path(entrypoint).suffix
    if runtime == "node" or suffix in {".js", ".cjs", ".mjs"}:
        return f"node {entrypoint}"
    if runtime == "python" or suffix == ".py":
        return f"python {entrypoint}"
    if runtime == "ruby" or suffix in {".rb", ".ru"}:
        return f"ruby {entrypoint}"
    return None


def entry_root(root: Any) -> str:
    root_value = string(root) or "."
    return root_value.lstrip("/") or "."
