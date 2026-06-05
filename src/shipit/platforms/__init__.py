from pathlib import Path
from typing import Optional

from shipit.platforms.base import (
    PlatformConfig,
    PlatformDetector,
    PlatformEntry,
    apply_platform_config,
)
from shipit.platforms.procfile import ProcfilePlatformDetector
from shipit.platforms.railway import RailwayPlatformDetector
from shipit.platforms.render import RenderPlatformDetector
from shipit.platforms.vercel import VercelPlatformDetector


def detect_platform_config(path: Path) -> Optional[PlatformConfig]:
    detected = all_platform_configs(path)
    if not detected:
        return None

    detected.sort(
        key=lambda config: (
            config.path.stat().st_mtime,
            str(config.path),
        ),
        reverse=True,
    )
    selected = detected[0]
    selected.selected_entry = select_platform_entry(selected)
    return selected


def all_platform_configs(path: Path) -> list[PlatformConfig]:
    detectors: list[type[PlatformDetector]] = [
        VercelPlatformDetector,
        RailwayPlatformDetector,
        RenderPlatformDetector,
        ProcfilePlatformDetector,
    ]
    configs: list[PlatformConfig] = []
    for detector in detectors:
        configs.extend(detector.detect_all(path))
    return configs


def select_platform_entry(
    platform_config: PlatformConfig,
) -> Optional[PlatformEntry]:
    entries = platform_config.entries
    if not entries:
        return None

    if platform_config.platform == "procfile":
        by_name = {entry.name: entry for entry in entries}
        for name in ("web", "default", "start"):
            entry = by_name.get(name)
            if entry and entry.is_runnable_web():
                return entry
        runnable = [entry for entry in entries if entry.is_runnable_web()]
        if len(runnable) == 1:
            return runnable[0]
        return None

    runnable = [entry for entry in entries if entry.is_runnable_web()]
    if not runnable:
        return None
    if len(runnable) > 1:
        platform_config.warnings.append(
            "Multiple runnable platform entries were detected; using the "
            f"first one: {runnable[0].name}"
        )
    return runnable[0]


__all__ = [
    "PlatformConfig",
    "PlatformDetector",
    "PlatformEntry",
    "all_platform_configs",
    "apply_platform_config",
    "detect_platform_config",
    "select_platform_entry",
]
