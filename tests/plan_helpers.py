"""Shared helper for plan-level test assertions.

Evaluates a project the way `shipit` would: detect the provider, load its
config, generate the two-line loader Shipit, and evaluate it through the
Starlark stdlib. Tests assert on the resulting Serve plan instead of on
generated source text.
"""

from pathlib import Path
from typing import Any, Optional, Tuple

from shipit.builders import LocalBuildBackend
from shipit.cli import ASSETS_PATH, Ctx, evaluate_shipit
from shipit.generator import (
    STARLARK_ENTRYPOINTS,
    generate_shipit_loader,
    load_provider,
    load_provider_config,
)
from shipit.providers.base import Config
from shipit.runners import LocalRunner
from shipit.shipit_types import RunStep, Serve


def evaluate_project_plan(
    workspace: Path,
    tmp_path: Path,
    config_overrides: Optional[dict] = None,
    subdir: Optional[str] = None,
    use_provider: Optional[str] = None,
) -> Tuple[LocalBuildBackend, Ctx, Serve, Any]:
    app_path = workspace / subdir if subdir else workspace
    base_config = Config()
    base_config.commands.enrich_from_path(app_path)
    provider_cls = load_provider(app_path, base_config, use_provider=use_provider)
    provider_config = load_provider_config(
        provider_cls, app_path, base_config, config=config_overrides
    )
    if subdir:
        provider_config.app_subdir = subdir
    loader_text = generate_shipit_loader(
        STARLARK_ENTRYPOINTS[provider_cls.name()], subdir=subdir
    )
    tmp_path.mkdir(parents=True, exist_ok=True)
    shipit_file = tmp_path / "Shipit.plan"
    shipit_file.write_text(loader_text)
    shipit_dir = tmp_path / ".shipit"
    backend = LocalBuildBackend(workspace, ASSETS_PATH, shipit_dir=shipit_dir)
    runner = LocalRunner(backend, workspace, shipit_dir=shipit_dir)
    ctx, serve = evaluate_shipit(
        shipit_file, backend, runner, provider_config, project_root=workspace
    )
    return backend, ctx, serve, provider_config


def run_commands(serve: Serve) -> list[str]:
    return [step.command for step in serve.build if isinstance(step, RunStep)]
