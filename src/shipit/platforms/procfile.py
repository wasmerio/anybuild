import re
from pathlib import Path

from shipit.platforms.base import PlatformConfig, PlatformEntry, PlatformName
from shipit.procfile import Procfile


class ProcfilePlatformDetector:
    name: PlatformName = "procfile"

    @classmethod
    def detect_all(cls, path: Path) -> list[PlatformConfig]:
        procfile_path = path / "Procfile"
        if not procfile_path.exists():
            return []
        procfile = Procfile.loads(procfile_path.read_text())
        release_command = procfile.processes.get("release")
        entries: list[PlatformEntry] = []
        for name, command in procfile.processes.items():
            if name == "release":
                continue
            entry_type = "worker" if _is_worker_process(name, command) else "web"
            unsupported = None
            if entry_type != "web":
                unsupported = (
                    "Procfile worker entries are detected but not applied "
                    "to provider config yet"
                )
            entries.append(
                PlatformEntry(
                    name=name,
                    type=entry_type,
                    start_command=command,
                    pre_deploy_command=release_command,
                    unsupported_reason=unsupported,
                )
            )
        return [PlatformConfig(cls.name, procfile_path, entries)]


def _is_worker_process(name: str, command: str) -> bool:
    if "worker" in name:
        return True
    first_token = re.split(r"\s+", command.strip(), maxsplit=1)[0]
    return first_token in {"celery", "dramatiq"}
