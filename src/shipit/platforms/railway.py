from pathlib import Path

from shipit.platforms.base import (
    PlatformConfig,
    PlatformEntry,
    PlatformName,
    list_commands,
    load_json,
    load_toml,
    string,
)


class RailwayPlatformDetector:
    name: PlatformName = "railway"
    ignored_dirs = {
        ".git",
        ".cache",
        ".next",
        ".vercel",
        ".venv",
        ".yarn",
        ".turbo",
        ".output",
        "node_modules",
    }

    @classmethod
    def detect_all(cls, path: Path) -> list[PlatformConfig]:
        return [cls._config_from_path(path, config) for config in cls._find_configs(path)]

    @classmethod
    def _find_configs(cls, path: Path, max_depth: int = 5) -> list[Path]:
        found: list[Path] = []
        for candidate in path.rglob("railway.*"):
            if candidate.name not in {"railway.json", "railway.toml"}:
                continue
            rel_parts = candidate.relative_to(path).parts
            if len(rel_parts) > max_depth + 1:
                continue
            if any(part in cls.ignored_dirs for part in rel_parts):
                continue
            found.append(candidate)
        return sorted(found)

    @classmethod
    def _config_from_path(
        cls,
        base_path: Path,
        config_path: Path,
    ) -> PlatformConfig:
        data = (
            load_json(config_path)
            if config_path.suffix == ".json"
            else load_toml(config_path)
        ) or {}
        build = data.get("build") if isinstance(data.get("build"), dict) else {}
        deploy = data.get("deploy") if isinstance(data.get("deploy"), dict) else {}
        root = config_path.parent.relative_to(base_path)
        root_text = str(root) if str(root) != "." else "."
        name = config_path.parent.name if root_text != "." else "default"
        schedule = string(deploy.get("cronSchedule"))
        start_command = string(deploy.get("startCommand"))
        unsupported = None
        if schedule and not start_command:
            unsupported = (
                "Railway cron entries need a start command before they can "
                "enrich provider config"
            )
        entry = PlatformEntry(
            name=name,
            type="cron" if schedule else "web",
            root=root_text,
            build_command=string(build.get("buildCommand")),
            start_command=start_command,
            pre_deploy_command=list_commands(deploy.get("preDeployCommand")),
            schedule=schedule,
            unsupported_reason=unsupported,
        )
        return PlatformConfig(cls.name, config_path, [entry])
