from pathlib import Path
from typing import Optional

from pydantic_settings import SettingsConfigDict

from .base import (
    Config,
    DetectResult,
    Provider,
    _exists,
)


class GoConfig(Config):
    model_config = SettingsConfigDict(extra="ignore", env_prefix="SHIPIT_")

    go_version: Optional[str] = "1.25.5"
    go_build_file: Optional[str] = None
    serve_binary: Optional[str] = None


class GoProvider(Provider):
    def __init__(self, path: Path, config: GoConfig):
        self.path = path
        self.config = config

    @classmethod
    def load_config(cls, path: Path, base_config: Config) -> GoConfig:
        config = GoConfig()
        if not config.go_build_file:
            build_file = cls.get_build_file(path)
            config.go_build_file = build_file
        if not config.go_build_file:
            raise Exception("No build file for go found")
        if not config.serve_binary:
            config.serve_binary = (
                config.go_build_file.replace("/", "_")
                .lower()
                .lstrip("_")
                .replace(".go", "")
            )
        if not config.serve_binary:
            raise Exception("No serve binary for go found")
        return config

    @classmethod
    def name(cls) -> str:
        return "go"

    @classmethod
    def get_build_file(cls, root_path: Path) -> str:
        paths_to_try = ["main.go", "server.go", "serve.go", "api.go", "web.go"]
        for path in paths_to_try:
            if "*" in path:
                continue  # This is for the glob finder
            if _exists(root_path, path):
                return path
            if _exists(root_path, f"src/{path}"):
                return f"src/{path}"
        for path in paths_to_try:
            found_path = next(root_path.glob(f"*/{path}"), None)
            if not found_path:
                found_path = next(root_path.glob(f"*/*/{path}"), None)
            if found_path:
                return str(found_path.relative_to(root_path))
        return None

    @classmethod
    def detect(cls, path: Path, config: Config) -> Optional[DetectResult]:
        if _exists(path, "go.mod", "go.sum"):
            return DetectResult(cls.name(), 80)
        return None

