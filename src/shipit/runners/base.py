from pathlib import Path
from typing import Dict, List, Protocol, TYPE_CHECKING

from shipit.builders.base import BuildBackend

if TYPE_CHECKING:
    from shipit.shipit_types import PrepareStep, Serve


class Runner(Protocol):
    build_backend: BuildBackend

    def prepare_config(self, provider_config: object) -> object: ...
    def build(self, serve: "Serve") -> None: ...
    def prepare(self, env: Dict[str, str], prepare: List["PrepareStep"]) -> None: ...
    def run_serve_command(self, command: str) -> None: ...
    def get_serve_mount_path(self, name: str) -> Path: ...
