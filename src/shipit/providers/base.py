
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Protocol, Literal
from shipit.procfile import Procfile


@dataclass
class DetectResult:
    name: str
    score: int  # Higher score wins when multiple providers match


@dataclass
class CustomCommands:
    install: Optional[str] = None
    build: Optional[str] = None
    start: Optional[str] = None
    after_deploy: Optional[str] = None

    # if (path / "Dockerfile").exists():
    #     # We get the start command from the Dockerfile
    #     with open(path / "Dockerfile", "r") as f:
    #         cmd = None
    #         for line in f:
    #             if line.startswith("CMD "):
    #                 cmd = line[4:].strip()
    #                 cmd = json.loads(cmd)
    #         # We get the last command
    #         if cmd:
    #             if isinstance(cmd, list):
    #                 cmd = " ".join(cmd)
    #             custom_commands.start = cmd

    @classmethod
    def from_path(cls, path: Path, use_procfile: bool = True) -> "CustomCommands":
        custom_commands = cls()
        if use_procfile:
            procfile_path = path / "Procfile"
            if procfile_path.exists():
                try:
                    procfile = Procfile.loads(procfile_path.read_text())
                    custom_commands.start = procfile.get_start_command()
                except Exception:
                    pass
        return custom_commands
    

class Provider(Protocol):
    def __init__(self, path: Path): ...
    @classmethod
    def name(cls) -> str: ...
    @classmethod
    def detect(cls, path: Path, custom_commands: CustomCommands) -> Optional[DetectResult]: ...
    # Structured plan steps (no path args; use self.path)
    def serve_name(self) -> Optional[str]: ...
    def dependencies(self) -> list["DependencySpec"]: ...
    def declarations(self) -> Optional[str]: ...
    def build_steps(self) -> list[str]: ...
    # Prepare: list of Starlark step calls (currently only run(...))
    def prepare_steps(self) -> Optional[List[str]]: ...
    def commands(self) -> Dict[str, str]: ...
    def services(self) -> List["ServiceSpec"]: ...
    def mounts(self) -> List["MountSpec"]: ...
    def volumes(self) -> List["VolumeSpec"]: ...
    def env(self) -> Optional[Dict[str, str]]: ...


@dataclass
class DependencySpec:
    name: str
    var_name: Optional[str] = None
    default_version: Optional[str] = None
    architecture_var_name: Optional[str] = None
    alias: Optional[str] = None  # Variable name in Shipit plan
    use_in_build: bool = False
    use_in_serve: bool = False


@dataclass
class MountSpec:
    name: str
    attach_to_build: bool = True
    attach_to_serve: bool = True


@dataclass
class VolumeSpec:
    name: str
    # Absolute path inside the serve/runtime environment where the volume is mounted
    serve_path: str
    var_name: Optional[str] = None


@dataclass
class ServiceSpec:
    name: str
    provider: Literal["postgres", "mysql", "redis"]


@dataclass
class ProviderPlan:
    serve_name: str
    provider: str
    mounts: List[MountSpec]
    platform: Optional[str] = None
    volumes: List[VolumeSpec] = field(default_factory=list)
    declarations: Optional[str] = None
    dependencies: List[DependencySpec] = field(default_factory=list)
    build_steps: List[str] = field(default_factory=list)
    prepare: Optional[List[str]] = None
    services: List[ServiceSpec] = field(default_factory=list)
    commands: Dict[str, str] = field(default_factory=dict)
    env: Optional[Dict[str, str]] = None


def _exists(path: Path, *candidates: str) -> bool:
    return any((path / c).exists() for c in candidates)
