from dataclasses import dataclass
from pathlib import Path
from typing import Optional, Protocol
from shipit.procfile import Procfile
from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


@dataclass
class DetectResult:
    name: str
    score: int  # Higher score wins when multiple providers match


class CustomCommands(BaseSettings):
    model_config = SettingsConfigDict(extra="ignore")

    install: Optional[str] = None
    build: Optional[str] = None
    start: Optional[str] = None
    after_deploy: Optional[str] = None

    def enrich_from_path(self, path: Path, use_procfile: bool = True) -> "CustomCommands":
        if use_procfile:
            procfile_path = path / "Procfile"
            if procfile_path.exists():
                try:
                    procfile = Procfile.loads(procfile_path.read_text())
                    self.start = procfile.get_start_command()
                except Exception:
                    pass
        return self


class Config(BaseSettings):
    model_config = SettingsConfigDict(extra="ignore", env_prefix="SHIPIT_")

    # Serve name; defaults to the project directory name (set by the CLI).
    name: Optional[str] = None
    port: Optional[int] = 8080
    commands: CustomCommands = Field(default_factory=CustomCommands)
    # Subdirectory of the workspace the app lives in (set by the CLI after
    # load; recorded in the generated Shipit file for subdir projects).
    app_subdir: Optional[str] = None


class Provider(Protocol):
    """A provider detects a project type and loads its resolved config.

    Plan construction lives in the Starlark stdlib (src/shipit/starlib);
    the generated Shipit file loads the provider's entrypoint and calls it
    with the config produced here.
    """

    def __init__(self, path: Path, config: Config): ...
    @classmethod
    def name(cls) -> str: ...
    @classmethod
    def load_config(cls, path: Path, config: Config) -> Config: ...
    @classmethod
    def detect(
        cls, path: Path, config: Config
    ) -> Optional[DetectResult]: ...


def _exists(path: Path, *candidates: str) -> bool:
    return any((path / c).exists() for c in candidates)
