from pathlib import Path
from typing import Any

from shipit.platforms.base import (
    PlatformConfig,
    PlatformEntry,
    PlatformName,
    entry_root,
    load_json,
    load_toml,
    runtime_start_command,
    string,
)


class VercelPlatformDetector:
    name: PlatformName = "vercel"

    @classmethod
    def detect_all(cls, path: Path) -> list[PlatformConfig]:
        configs: list[PlatformConfig] = []
        for config_path in (path / "vercel.json", path / "vercel.toml"):
            if not config_path.exists():
                continue
            configs.append(cls._detect_file(config_path))
        return configs

    @classmethod
    def _detect_file(cls, config_path: Path) -> PlatformConfig:
        data = (
            load_toml(config_path)
            if config_path.suffix == ".toml"
            else load_json(config_path)
        )
        if data is None:
            return PlatformConfig(
                cls.name,
                config_path,
                [],
                warnings=[f"Could not parse {config_path.name}"],
            )

        raw_entries = data.get("services") or data.get("experimentalServices")
        if isinstance(raw_entries, dict):
            entries = [
                cls._entry_from_config(name, value)
                for name, value in raw_entries.items()
                if isinstance(value, dict)
            ]
        else:
            entries = [
                PlatformEntry(
                    name="default",
                    install_command=string(data.get("installCommand")),
                    build_command=string(data.get("buildCommand")),
                )
            ]
        return PlatformConfig(cls.name, config_path, entries)

    @classmethod
    def _entry_from_config(
        cls,
        name: str,
        value: dict[str, Any],
    ) -> PlatformEntry:
        runtime = string(value.get("runtime"))
        entrypoint = string(value.get("entrypoint"))
        entry_type = string(value.get("type")) or "web"
        unsupported = None
        if entry_type not in {"web", "static"}:
            unsupported = (
                f"Vercel {entry_type} entries are detected but not applied "
                "to provider config yet"
            )
        return PlatformEntry(
            name=name,
            type=entry_type,
            root=entry_root(value.get("root")),
            runtime=runtime,
            framework=string(value.get("framework")),
            entrypoint=entrypoint,
            install_command=string(value.get("installCommand")),
            build_command=string(value.get("buildCommand")),
            start_command=runtime_start_command(runtime, entrypoint),
            pre_deploy_command=string(value.get("preDeployCommand")),
            schedule=string(value.get("schedule")),
            unsupported_reason=unsupported,
        )
