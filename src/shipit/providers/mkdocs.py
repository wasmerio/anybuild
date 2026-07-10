from pathlib import Path
from typing import Optional

from .base import DetectResult, _exists, Config
from .staticfile import StaticFileProvider, StaticFileConfig
from .python import PythonProvider, PythonConfig
from pydantic_settings import SettingsConfigDict


class MkdocsConfig(PythonConfig, StaticFileConfig):
    model_config = SettingsConfigDict(extra="ignore", env_prefix="SHIPIT_")

    mkdocs_version: Optional[str] = None


class MkdocsProvider(StaticFileProvider):
    @classmethod
    def load_config(cls, path: Path, base_config: Config) -> MkdocsConfig:
        python_config = PythonProvider.load_config(
            path, base_config, must_have_deps={"mkdocs"}
        )
        staticfile_config = StaticFileProvider.load_config(path, base_config)

        return MkdocsConfig(
            **(
                python_config.model_dump()
                | staticfile_config.model_dump()
                | base_config.model_dump()
            )
        )

    @classmethod
    def name(cls) -> str:
        return "mkdocs"

    @classmethod
    def detect(cls, path: Path, config: Config) -> Optional[DetectResult]:
        if _exists(path, "mkdocs.yml", "mkdocs.yaml"):
            return DetectResult(cls.name(), 85)
        if config.commands.build and config.commands.build.startswith("mkdocs "):
            return DetectResult(cls.name(), 85)
        return None

