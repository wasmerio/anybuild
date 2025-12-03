from pathlib import Path
from typing import Dict, List, Optional, Protocol, TYPE_CHECKING

if TYPE_CHECKING:
    from shipit.cli import Mount, Step


class BuildBackend(Protocol):
    def build(
        self, name: str, env: Dict[str, str], mounts: List["Mount"], steps: List["Step"]
    ) -> None: ...
    def get_build_mount_path(self, name: str) -> Path: ...
    def get_artifact_mount_path(self, name: str) -> Path: ...
    def get_runtime_path(self) -> Optional[str]: ...
