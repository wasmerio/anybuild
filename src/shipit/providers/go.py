from pathlib import Path
from typing import Dict, Optional

from .base import (
    DetectResult,
    DependencySpec,
    Provider,
    _exists,
    ServiceSpec,
    VolumeSpec,
    MountSpec,
    Config,
)
from .base import Config, Provider
from pydantic_settings import SettingsConfigDict


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
            config.serve_binary = config.go_build_file.replace("/", "_").lower().lstrip("_").replace(".go", "")
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

    def serve_name(self) -> Optional[str]:
        return None

    def dependencies(self) -> list[DependencySpec]:
        return [
            DependencySpec(
                "go",
                var_name="config.go_version",
                use_in_build=True,
            ),
        ]

    def build_steps(self) -> list[str]:
        return [
            "workdir(temp.path)",
            "use(go)",
            'copy(".", ".", ignore=[".git"])',
            'env(GOCACHE="/tmp/.cache/go-build", GOPATH=temp.path)',
            'run("go build -o {} {}".format(config.serve_binary, config.go_build_file), group="build")',
            'run("cp {}/{} {}/{}".format(temp.path, config.serve_binary, app.path, config.serve_binary))',
        ]

    def mounts(self) -> list[MountSpec]:
        return [
            MountSpec("temp", attach_to_serve=False),
            MountSpec("app", attach_to_serve=True),
        ]

    def commands(self) -> Dict[str, str]:
        return {
            "start": '"{}/{}".format(app.serve_path, config.serve_binary)',
        }

    def services(self) -> list[ServiceSpec]:
        return []

    def volumes(self) -> list[VolumeSpec]:
        return []
