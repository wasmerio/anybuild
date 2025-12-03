from __future__ import annotations

from pathlib import Path
from typing import Any, Dict, List, Optional, Protocol, TYPE_CHECKING

if TYPE_CHECKING:
    from shipit.cli import Mount, Serve, Step


class BuildBackend(Protocol):
    def build(
        self, env: Dict[str, str], mounts: List["Mount"], steps: List["Step"]
    ) -> None: ...
    def finalize_build(self, serve: "Serve") -> None: ...
    def get_build_mount_path(self, name: str) -> Path: ...
    def get_artifact_mount_path(self, name: str) -> Path: ...
    def get_runtime_path(self) -> Optional[str]: ...
