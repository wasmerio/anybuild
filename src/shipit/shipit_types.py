from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Literal, Optional, Union


@dataclass
class Mount:
    name: str
    build_path: Path
    serve_path: Path


@dataclass
class Volume:
    name: str
    serve_path: Path


@dataclass
class Service:
    name: str
    provider: Literal["postgres", "mysql", "redis"]


@dataclass
class Serve:
    name: str
    provider: str
    build: List["Step"]
    deps: List["Package"]
    commands: Dict[str, str]
    cwd: Optional[str] = None
    prepare: Optional[List["PrepareStep"]] = None
    workers: Optional[List[str]] = None
    mounts: Optional[List[Mount]] = None
    volumes: Optional[List[Volume]] = None
    env: Optional[Dict[str, str]] = None
    services: Optional[List[Service]] = None


@dataclass
class Package:
    name: str
    version: Optional[str] = None
    architecture: Optional[Literal["64-bit", "32-bit"]] = None

    def __str__(self) -> str:
        name = f"{self.name}({self.architecture})" if self.architecture else self.name
        if self.version is None:
            return name
        return f"{name}@{self.version}"


@dataclass
class RunStep:
    command: str
    inputs: Optional[List[str]] = None
    outputs: Optional[List[str]] = None
    group: Optional[str] = None


@dataclass
class WorkdirStep:
    path: Path


@dataclass
class CopyStep:
    source: str
    target: str
    ignore: Optional[List[str]] = None
    base: Literal["source", "assets"] = "source"

    def is_download(self) -> bool:
        return self.source.startswith("http://") or self.source.startswith("https://")


@dataclass
class EnvStep:
    variables: Dict[str, str]

    def __str__(self) -> str:
        return " ".join([f"{key}={value}" for key, value in self.variables.items()])


@dataclass
class UseStep:
    dependencies: List[Package]


@dataclass
class PathStep:
    path: str


Step = Union[RunStep, CopyStep, EnvStep, PathStep, UseStep, WorkdirStep]
PrepareStep = Union[RunStep]


@dataclass
class Build:
    deps: List[Package]
    steps: List[Step]
