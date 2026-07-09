"""Legacy inline generator vs Starlark stdlib: plan equivalence.

For every example whose provider has been ported to the Starlark stdlib,
evaluate both the legacy fully-inlined Shipit text and the new two-line
loader form with the same config, and assert the resulting plans are
identical. A provider's legacy emitters may only be deleted once its
examples all pass through both paths byte-identically.
"""

import dataclasses
from pathlib import Path
from typing import Any

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
    generate_shipit_inline,
    generate_shipit_loader,
    load_provider,
    load_provider_config,
)
from shipit.providers.base import Config
from shipit.runners import LocalRunner

ROOT = Path(__file__).resolve().parent.parent
EXAMPLES = ROOT / "examples"

# Env required for detection/config of specific examples (mirrors the
# generate-examples golden test).
EXAMPLE_ENV = {
    "php-wordpress-empty": {"SHIPIT_WP_VERSION": "latest", "SHIPIT_PHPIX": "true"},
}

# Examples whose app lives in a subdirectory of the workspace (mirrors the
# generate-examples golden test).
SUBDIR_EXAMPLES = {
    "node-npm-file-subdir": "apps/dashboard",
    "node-pnpm-workspace-subdir": "apps/dashboard",
}


def _example_env(example: Path):
    import contextlib
    import os

    @contextlib.contextmanager
    def _ctx():
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

    return _ctx()


def _ported_examples() -> list[Path]:
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


_PORTED_EXAMPLES = _ported_examples()


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
        "workers": list(serve.workers or []),
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


def _evaluate_text(
    text: str, workspace: Path, provider_config: Config, tmp_dir: Path, shipit_dir: Path
) -> dict:
    tmp_dir.mkdir(parents=True, exist_ok=True)
    shipit_file = tmp_dir / "Shipit"
    shipit_file.write_text(text)
    backend = LocalBuildBackend(workspace, ASSETS_PATH, shipit_dir=shipit_dir)
    runner = LocalRunner(backend, workspace, shipit_dir=shipit_dir)
    ctx, serve = evaluate_shipit(
        shipit_file, backend, runner, provider_config, project_root=workspace
    )
    return _normalize(ctx, serve)


@pytest.mark.parametrize(
    "example_dir", _PORTED_EXAMPLES, ids=[p.name for p in _PORTED_EXAMPLES]
)
def test_plan_equivalence(
    example_dir: Path, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    for key, value in EXAMPLE_ENV.get(example_dir.name, {}).items():
        monkeypatch.setenv(key, value)
    subdir = SUBDIR_EXAMPLES.get(example_dir.name)
    app_path = example_dir / subdir if subdir else example_dir
    base_config = Config()
    base_config.commands.enrich_from_path(app_path)

    provider_cls = load_provider(app_path, base_config)
    provider_config = load_provider_config(provider_cls, app_path, base_config)
    project_paths = ProjectPaths(example_dir, app_path, subdir)
    apply_subdir_provider_config(project_paths, provider_config)
    apply_subdir_workspace_config(project_paths, provider_config)
    provider = provider_cls(app_path, provider_config)

    legacy_text = generate_shipit_inline(app_path, provider, subdir=subdir)
    loader_text = generate_shipit_loader(
        STARLARK_ENTRYPOINTS[provider_cls.name()], subdir=subdir
    )

    # Shared shipit_dir so mount paths are identical across both evaluations.
    shipit_dir = tmp_path / ".shipit"
    legacy_plan = _evaluate_text(
        legacy_text, example_dir, provider_config, tmp_path / "legacy", shipit_dir
    )
    loader_plan = _evaluate_text(
        loader_text, example_dir, provider_config, tmp_path / "loader", shipit_dir
    )

    assert loader_plan == legacy_plan


@pytest.mark.parametrize(
    "example_dir", _PORTED_EXAMPLES, ids=[p.name for p in _PORTED_EXAMPLES]
)
def test_plan_equivalence_cross_platform(
    example_dir: Path, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Same as test_plan_equivalence, with cross_platform set the way
    WasmerRunner.prepare_config does — this activates the cross-wheel build
    paths that plain local evaluation never reaches."""
    for key, value in EXAMPLE_ENV.get(example_dir.name, {}).items():
        monkeypatch.setenv(key, value)
    subdir = SUBDIR_EXAMPLES.get(example_dir.name)
    app_path = example_dir / subdir if subdir else example_dir
    base_config = Config()
    base_config.commands.enrich_from_path(app_path)

    provider_cls = load_provider(app_path, base_config)
    provider_config = load_provider_config(provider_cls, app_path, base_config)
    if "cross_platform" not in type(provider_config).model_fields:
        pytest.skip("provider has no cross_platform config")
    provider_config.cross_platform = "wasix_wasm32"
    project_paths = ProjectPaths(example_dir, app_path, subdir)
    apply_subdir_provider_config(project_paths, provider_config)
    apply_subdir_workspace_config(project_paths, provider_config)
    provider = provider_cls(app_path, provider_config)

    legacy_text = generate_shipit_inline(app_path, provider, subdir=subdir)
    loader_text = generate_shipit_loader(
        STARLARK_ENTRYPOINTS[provider_cls.name()], subdir=subdir
    )

    shipit_dir = tmp_path / ".shipit"
    legacy_plan = _evaluate_text(
        legacy_text, example_dir, provider_config, tmp_path / "legacy", shipit_dir
    )
    loader_plan = _evaluate_text(
        loader_text, example_dir, provider_config, tmp_path / "loader", shipit_dir
    )
    assert loader_plan == legacy_plan


def _assert_subdir_equivalence(workspace: Path, app_path: Path, tmp_path: Path) -> None:
    subdir = str(app_path.relative_to(workspace))
    base_config = Config()
    base_config.commands.enrich_from_path(app_path)
    provider_cls = load_provider(app_path, base_config)
    assert provider_cls.name() in STARLARK_ENTRYPOINTS
    provider_config = load_provider_config(provider_cls, app_path, base_config)
    provider_config.app_subdir = subdir
    provider = provider_cls(app_path, provider_config)

    legacy_text = generate_shipit_inline(app_path, provider, subdir=subdir)
    loader_text = generate_shipit_loader(
        STARLARK_ENTRYPOINTS[provider_cls.name()], subdir=subdir
    )

    shipit_dir = tmp_path / ".shipit"
    legacy_plan = _evaluate_text(
        legacy_text, workspace, provider_config, tmp_path / "legacy", shipit_dir
    )
    loader_plan = _evaluate_text(
        loader_text, workspace, provider_config, tmp_path / "loader", shipit_dir
    )
    assert loader_plan == legacy_plan


def test_plan_equivalence_python_subdir(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    app_path = workspace / "apps" / "site"
    app_path.mkdir(parents=True)
    (app_path / "requirements.txt").write_text("click==8.1.7\n")
    (app_path / "main.py").write_text("print('ok')\n")

    _assert_subdir_equivalence(workspace, app_path, tmp_path)


def test_plan_equivalence_static_subdir(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    app_path = workspace / "apps" / "site"
    public = app_path / "public"
    public.mkdir(parents=True)
    (public / "index.html").write_text("<h1>ok</h1>\n")

    _assert_subdir_equivalence(workspace, app_path, tmp_path)


def test_plan_equivalence_go_subdir(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    app_path = workspace / "apps" / "site"
    app_path.mkdir(parents=True)
    (app_path / "go.mod").write_text("module example.com/site\n\ngo 1.25\n")
    (app_path / "main.go").write_text("package main\nfunc main() {}\n")

    _assert_subdir_equivalence(workspace, app_path, tmp_path)


def _assert_workspace_equivalence(workspace: Path, tmp_path: Path) -> None:
    base_config = Config()
    base_config.commands.enrich_from_path(workspace)
    provider_cls = load_provider(workspace, base_config)
    assert provider_cls.name() in STARLARK_ENTRYPOINTS
    provider_config = load_provider_config(provider_cls, workspace, base_config)
    provider = provider_cls(workspace, provider_config)

    legacy_text = generate_shipit_inline(workspace, provider, subdir=None)
    loader_text = generate_shipit_loader(
        STARLARK_ENTRYPOINTS[provider_cls.name()], subdir=None
    )

    shipit_dir = tmp_path / ".shipit"
    legacy_plan = _evaluate_text(
        legacy_text, workspace, provider_config, tmp_path / "legacy", shipit_dir
    )
    loader_plan = _evaluate_text(
        loader_text, workspace, provider_config, tmp_path / "loader", shipit_dir
    )
    assert loader_plan == legacy_plan


def test_plan_equivalence_jekyll_with_gemfile(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    (workspace / "_config.yml").write_text("title: Test\n")
    (workspace / "Gemfile").write_text('source "https://rubygems.org"\ngem "jekyll"\n')
    (workspace / "Gemfile.lock").write_text("GEM\n")
    (workspace / "index.md").write_text("# hi\n")

    _assert_workspace_equivalence(workspace, tmp_path)


def test_plan_equivalence_jekyll_without_gemfile(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    (workspace / "_config.yml").write_text("title: Test\ndestination: out\n")
    (workspace / "index.md").write_text("# hi\n")
    (workspace / "_redirects").write_text("/old /new 301\n")

    _assert_workspace_equivalence(workspace, tmp_path)
