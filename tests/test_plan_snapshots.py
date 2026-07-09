"""Golden plan snapshots for every example.

The snapshots were captured from plans proven byte-identical to the legacy
string-emitting generator (via the plan-equivalence suite) before that
generator was removed. They now gate any change to the Starlark stdlib or
the evaluation engine: an unintended plan change shows up as a snapshot
diff, example by example.

Refresh intentionally with:

    UPDATE_PLAN_SNAPSHOTS=1 pytest tests/test_plan_snapshots.py
"""

import contextlib
import dataclasses
import json
import os
from pathlib import Path
from typing import Any, Iterator, Optional

import pytest

from shipit.builders import LocalBuildBackend
from shipit.cli import (
    ASSETS_PATH,
    ProjectPaths,
    apply_subdir_provider_config,
    apply_subdir_workspace_config,
    evaluate_shipit,
)
from shipit.generator import (
    STARLARK_ENTRYPOINTS,
    generate_shipit_loader,
    load_provider,
    load_provider_config,
)
from shipit.providers.base import Config
from shipit.runners import LocalRunner

ROOT = Path(__file__).resolve().parent.parent
EXAMPLES = ROOT / "examples"
SNAPSHOTS = ROOT / "tests" / "plan_snapshots"

# Env required for detection/config of specific examples (mirrors the
# generate-examples golden test).
EXAMPLE_ENV = {
    "php-wordpress-empty": {"SHIPIT_WP_VERSION": "latest", "SHIPIT_PHPIX": "true"},
}

# Examples whose app lives in a subdirectory of the workspace.
SUBDIR_EXAMPLES = {
    "node-npm-file-subdir": "apps/dashboard",
    "node-pnpm-workspace-subdir": "apps/dashboard",
}


@contextlib.contextmanager
def _example_env(example: Path) -> Iterator[None]:
    overrides = EXAMPLE_ENV.get(example.name, {})
    saved = {key: os.environ.get(key) for key in overrides}
    os.environ.update(overrides)
    try:
        yield
    finally:
        for key, value in saved.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value


def _all_examples() -> list[Path]:
    out = []
    for example in sorted(EXAMPLES.iterdir()):
        if not example.is_dir():
            continue
        subdir = SUBDIR_EXAMPLES.get(example.name)
        app_path = example / subdir if subdir else example
        with _example_env(example):
            base_config = Config()
            base_config.commands.enrich_from_path(app_path)
            try:
                provider_cls = load_provider(app_path, base_config)
            except Exception:
                continue
            if provider_cls.name() in STARLARK_ENTRYPOINTS:
                out.append(example)
    return out


_ALL_EXAMPLES = _all_examples()


def _jsonable(value: Any) -> Any:
    if isinstance(value, dict):
        return {key: _jsonable(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_jsonable(item) for item in value]
    if isinstance(value, Path):
        return str(value)
    return value


def _norm_step(step: Any) -> Any:
    data = dataclasses.asdict(step)
    data["__type__"] = type(step).__name__
    return _jsonable(data)


def _normalize(ctx: Any, serve: Any) -> dict:
    return {
        "name": serve.name,
        "provider": serve.provider,
        "cwd": serve.cwd,
        "build": [_norm_step(step) for step in serve.build],
        "deps": [str(dep) for dep in serve.deps],
        "commands": dict(serve.commands),
        "prepare": [_norm_step(step) for step in (serve.prepare or [])],
        "env": dict(serve.env or {}),
        "mounts": _jsonable(
            [dataclasses.asdict(mount) for mount in (serve.mounts or [])]
        ),
        "volumes": _jsonable(
            [dataclasses.asdict(volume) for volume in (serve.volumes or [])]
        ),
        "services": _jsonable(
            [dataclasses.asdict(service) for service in (serve.services or [])]
        ),
        "ctx_mounts": _jsonable([dataclasses.asdict(m) for m in ctx.mounts]),
        "ctx_volumes": _jsonable([dataclasses.asdict(v) for v in ctx.volumes]),
        "ctx_services": {
            key: dataclasses.asdict(service) for key, service in ctx.services.items()
        },
        "ctx_packages": {key: str(pkg) for key, pkg in ctx.packages.items()},
    }


def _evaluate_plan(
    workspace: Path,
    provider_config: Config,
    loader_text: str,
    tmp_path: Path,
) -> dict:
    work_dir = tmp_path / "eval"
    work_dir.mkdir(parents=True, exist_ok=True)
    shipit_file = work_dir / "Shipit"
    shipit_file.write_text(loader_text)
    shipit_dir = tmp_path / ".shipit"
    backend = LocalBuildBackend(workspace, ASSETS_PATH, shipit_dir=shipit_dir)
    runner = LocalRunner(backend, workspace, shipit_dir=shipit_dir)
    ctx, serve = evaluate_shipit(
        shipit_file, backend, runner, provider_config, project_root=workspace
    )
    plan = _normalize(ctx, serve)
    # Tokenize machine-specific roots so snapshots are stable everywhere.
    text = json.dumps(plan, indent=1, sort_keys=True)
    text = text.replace(str(shipit_dir), "<SHIPIT_DIR>")
    text = text.replace(str(workspace), "<WORKSPACE>")
    return text + "\n"


def _check_snapshot(snapshot_name: str, plan_text: str) -> None:
    path = SNAPSHOTS / f"{snapshot_name}.json"
    if os.environ.get("UPDATE_PLAN_SNAPSHOTS"):
        SNAPSHOTS.mkdir(parents=True, exist_ok=True)
        path.write_text(plan_text)
        return
    assert path.is_file(), (
        f"Missing plan snapshot {path.name}. "
        "Run UPDATE_PLAN_SNAPSHOTS=1 pytest tests/test_plan_snapshots.py"
    )
    expected = path.read_text()
    assert plan_text == expected, (
        f"Plan for {snapshot_name} diverged from its snapshot. If the change "
        "is intentional, refresh with UPDATE_PLAN_SNAPSHOTS=1"
    )


def _example_plan(
    example_dir: Path,
    tmp_path: Path,
    cross_platform: Optional[str] = None,
) -> Optional[str]:
    subdir = SUBDIR_EXAMPLES.get(example_dir.name)
    app_path = example_dir / subdir if subdir else example_dir
    base_config = Config()
    base_config.commands.enrich_from_path(app_path)

    provider_cls = load_provider(app_path, base_config)
    provider_config = load_provider_config(provider_cls, app_path, base_config)
    if cross_platform is not None:
        if "cross_platform" not in type(provider_config).model_fields:
            return None
        provider_config.cross_platform = cross_platform
    project_paths = ProjectPaths(example_dir, app_path, subdir)
    apply_subdir_provider_config(project_paths, provider_config)
    apply_subdir_workspace_config(project_paths, provider_config)

    loader_text = generate_shipit_loader(
        STARLARK_ENTRYPOINTS[provider_cls.name()], subdir=subdir
    )
    return _evaluate_plan(example_dir, provider_config, loader_text, tmp_path)


@pytest.mark.parametrize(
    "example_dir", _ALL_EXAMPLES, ids=[p.name for p in _ALL_EXAMPLES]
)
def test_plan_snapshot(
    example_dir: Path, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    for key, value in EXAMPLE_ENV.get(example_dir.name, {}).items():
        monkeypatch.setenv(key, value)
    plan_text = _example_plan(example_dir, tmp_path)
    assert plan_text is not None
    _check_snapshot(example_dir.name, plan_text)


@pytest.mark.parametrize(
    "example_dir", _ALL_EXAMPLES, ids=[p.name for p in _ALL_EXAMPLES]
)
def test_plan_snapshot_cross_platform(
    example_dir: Path, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Same plans with cross_platform set the way WasmerRunner does — this
    activates the cross-wheel build paths plain evaluation never reaches."""
    for key, value in EXAMPLE_ENV.get(example_dir.name, {}).items():
        monkeypatch.setenv(key, value)
    plan_text = _example_plan(example_dir, tmp_path, cross_platform="wasix_wasm32")
    if plan_text is None:
        pytest.skip("provider has no cross_platform config")
    _check_snapshot(f"{example_dir.name}__cross", plan_text)


def _workspace_plan(workspace: Path, app_path: Path, tmp_path: Path) -> str:
    subdir = (
        str(app_path.relative_to(workspace)) if app_path != workspace else None
    )
    base_config = Config()
    base_config.commands.enrich_from_path(app_path)
    provider_cls = load_provider(app_path, base_config)
    assert provider_cls.name() in STARLARK_ENTRYPOINTS
    provider_config = load_provider_config(provider_cls, app_path, base_config)
    if subdir:
        provider_config.app_subdir = subdir
    loader_text = generate_shipit_loader(
        STARLARK_ENTRYPOINTS[provider_cls.name()], subdir=subdir
    )
    return _evaluate_plan(workspace, provider_config, loader_text, tmp_path)


def test_plan_snapshot_python_subdir(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    app_path = workspace / "apps" / "site"
    app_path.mkdir(parents=True)
    (app_path / "requirements.txt").write_text("click==8.1.7\n")
    (app_path / "main.py").write_text("print('ok')\n")

    _check_snapshot(
        "synthetic__python_subdir", _workspace_plan(workspace, app_path, tmp_path)
    )


def test_plan_snapshot_static_subdir(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    app_path = workspace / "apps" / "site"
    public = app_path / "public"
    public.mkdir(parents=True)
    (public / "index.html").write_text("<h1>ok</h1>\n")

    _check_snapshot(
        "synthetic__static_subdir", _workspace_plan(workspace, app_path, tmp_path)
    )


def test_plan_snapshot_go_subdir(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    app_path = workspace / "apps" / "site"
    app_path.mkdir(parents=True)
    (app_path / "go.mod").write_text("module example.com/site\n\ngo 1.25\n")
    (app_path / "main.go").write_text("package main\nfunc main() {}\n")

    _check_snapshot(
        "synthetic__go_subdir", _workspace_plan(workspace, app_path, tmp_path)
    )


def test_plan_snapshot_jekyll_with_gemfile(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    (workspace / "_config.yml").write_text("title: Test\n")
    (workspace / "Gemfile").write_text('source "https://rubygems.org"\ngem "jekyll"\n')
    (workspace / "Gemfile.lock").write_text("GEM\n")
    (workspace / "index.md").write_text("# hi\n")

    _check_snapshot(
        "synthetic__jekyll_gemfile", _workspace_plan(workspace, workspace, tmp_path)
    )


def test_plan_snapshot_jekyll_without_gemfile(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    (workspace / "_config.yml").write_text("title: Test\ndestination: out\n")
    (workspace / "index.md").write_text("# hi\n")
    (workspace / "_redirects").write_text("/old /new 301\n")

    _check_snapshot(
        "synthetic__jekyll_bare", _workspace_plan(workspace, workspace, tmp_path)
    )
