from pathlib import Path
from typing import Any

from shipit.platforms.base import (
    PlatformConfig,
    PlatformEntry,
    PlatformName,
    entry_root,
    load_yaml,
    string,
)


class RenderPlatformDetector:
    name: PlatformName = "render"

    @classmethod
    def detect_all(cls, path: Path) -> list[PlatformConfig]:
        config_path = path / "render.yaml"
        if not config_path.exists():
            return []

        data = load_yaml(config_path)
        if data is None:
            return [
                PlatformConfig(
                    cls.name,
                    config_path,
                    [],
                    warnings=["Could not parse render.yaml"],
                )
            ]
        raw_services = data.get("services")
        if not isinstance(raw_services, list):
            return [PlatformConfig(cls.name, config_path, [])]
        entries = [
            cls._entry_from_config(index, service)
            for index, service in enumerate(raw_services)
            if isinstance(service, dict)
        ]
        return [PlatformConfig(cls.name, config_path, entries)]

    @classmethod
    def _entry_from_config(
        cls,
        index: int,
        value: dict[str, Any],
    ) -> PlatformEntry:
        entry_type = string(value.get("type")) or "web"
        name = string(value.get("name")) or f"{entry_type}-{index + 1}"
        unsupported = None
        if entry_type not in {"web", "static"}:
            unsupported = (
                f"Render {entry_type} entries are detected but not applied "
                "to provider config yet"
            )
        return PlatformEntry(
            name=name,
            type=entry_type,
            root=entry_root(value.get("rootDir")),
            runtime=string(value.get("runtime")),
            build_command=string(value.get("buildCommand")),
            start_command=string(value.get("startCommand")),
            pre_deploy_command=string(value.get("preDeployCommand")),
            schedule=string(value.get("schedule")),
            unsupported_reason=unsupported,
        )
