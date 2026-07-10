"""Dump evaluation fixtures for the Rust snapshot gate.

Produces a manifest of every plan-snapshot case (examples, cross-platform
variants, synthetic workspaces) with the provider config in its JSON view
form and the loader Shipit text — everything the Rust evaluation host
needs to reproduce `tests/plan_snapshots/*.json` byte-for-byte without
the Python detection layer.

Usage: uv run python scripts/dump_rust_fixtures.py [out_dir]
"""

import json
import os
import shutil
import subprocess
import sys
from contextlib import contextmanager
from pathlib import Path

from pydantic import BaseModel

from shipit.cli import (
    ProjectPaths,
    apply_subdir_provider_config,
    apply_subdir_workspace_config,
)
from shipit.generator import (
    STARLARK_ENTRYPOINTS,
    generate_shipit_loader,
    load_provider,
    load_provider_config,
)
from shipit.providers.base import Config

ROOT = Path(__file__).resolve().parent.parent
EXAMPLES = ROOT / "examples"

EXAMPLE_ENV = {
    "php-wordpress-empty": {"SHIPIT_WP_VERSION": "latest", "SHIPIT_PHPIX": "true"},
}
SUBDIR_EXAMPLES = {
    "node-npm-file-subdir": "apps/dashboard",
    "node-pnpm-workspace-subdir": "apps/dashboard",
}


@contextmanager
def _example_env(name: str):
    overrides = EXAMPLE_ENV.get(name, {})
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


def config_json(config):
    """The JSON form of config_view: model_dump(mode="json") with set-typed
    fields sorted, recursively."""

    def view(dumped, live):
        if isinstance(live, BaseModel) and isinstance(dumped, dict):
            return {
                key: view(value, getattr(live, key, None))
                for key, value in dumped.items()
            }
        if isinstance(dumped, dict):
            return dict(dumped)
        if isinstance(live, (set, frozenset)):
            return sorted(dumped)
        if isinstance(dumped, list):
            return list(dumped)
        return dumped

    return view(config.model_dump(mode="json"), config)


def _example_case(example_dir: Path, cross_platform=None):
    subdir = SUBDIR_EXAMPLES.get(example_dir.name)
    app_path = example_dir / subdir if subdir else example_dir
    base_config = Config()
    base_config.commands.enrich_from_path(app_path)
    try:
        provider_cls = load_provider(app_path, base_config)
    except Exception:
        return None
    if provider_cls.name() not in STARLARK_ENTRYPOINTS:
        return None
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
    name = example_dir.name + ("__cross" if cross_platform else "")
    case = {
        "name": name,
        "workspace": str(example_dir),
        "subdir": subdir,
        "shipit": loader_text,
        "config": config_json(provider_config),
    }
    if cross_platform is None:
        legacy = _legacy_shipit(example_dir.name)
        if legacy:
            case["legacy_shipit"] = legacy
    return case


_LEGACY_REF: str | None | bool = False  # unset sentinel


def _legacy_ref() -> str | None:
    """The last commit whose example Shipit files were fully inlined: the
    parent of the commit that introduced the load()-based format."""
    global _LEGACY_REF
    if _LEGACY_REF is not False:
        return _LEGACY_REF
    result = subprocess.run(
        [
            "git", "log", "--format=%H", "--all", "--reverse",
            "-S", 'load("//shipit',
            "--", "examples/python-flask/Shipit",
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    intro = result.stdout.split()[0] if result.stdout.split() else None
    _LEGACY_REF = f"{intro}^" if intro else None
    return _LEGACY_REF


def _legacy_shipit(example_name: str) -> str | None:
    """The fully-inlined Shipit file this example had pre-stdlib, if any."""
    ref = _legacy_ref()
    if not ref:
        return None
    result = subprocess.run(
        ["git", "show", f"{ref}:examples/{example_name}/Shipit"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0 or 'load("//shipit' in result.stdout:
        return None
    return result.stdout


def _workspace_case(name: str, workspace: Path, app_path: Path):
    subdir = (
        str(app_path.relative_to(workspace)) if app_path != workspace else None
    )
    base_config = Config()
    base_config.commands.enrich_from_path(app_path)
    provider_cls = load_provider(app_path, base_config)
    provider_config = load_provider_config(provider_cls, app_path, base_config)
    if subdir:
        provider_config.app_subdir = subdir
    loader_text = generate_shipit_loader(
        STARLARK_ENTRYPOINTS[provider_cls.name()], subdir=subdir
    )
    return {
        "name": name,
        "workspace": str(workspace),
        "subdir": subdir,
        "shipit": loader_text,
        "config": config_json(provider_config),
    }


def _synthetic_cases(fixtures: Path):
    """Materialize the synthetic workspaces from test_plan_snapshots.py."""
    out = []
    base = fixtures / "synthetic"
    if base.exists():
        shutil.rmtree(base)

    ws = base / "python_subdir" / "workspace"
    app = ws / "apps" / "site"
    app.mkdir(parents=True)
    (app / "requirements.txt").write_text("click==8.1.7\n")
    (app / "main.py").write_text("print('ok')\n")
    out.append(_workspace_case("synthetic__python_subdir", ws, app))

    ws = base / "static_subdir" / "workspace"
    app = ws / "apps" / "site"
    (app / "public").mkdir(parents=True)
    (app / "public" / "index.html").write_text("<h1>ok</h1>\n")
    out.append(_workspace_case("synthetic__static_subdir", ws, app))

    ws = base / "go_subdir" / "workspace"
    app = ws / "apps" / "site"
    app.mkdir(parents=True)
    (app / "go.mod").write_text("module example.com/site\n\ngo 1.25\n")
    (app / "main.go").write_text("package main\nfunc main() {}\n")
    out.append(_workspace_case("synthetic__go_subdir", ws, app))

    ws = base / "jekyll_gemfile" / "workspace"
    ws.mkdir(parents=True)
    (ws / "_config.yml").write_text("title: Test\n")
    (ws / "Gemfile").write_text('source "https://rubygems.org"\ngem "jekyll"\n')
    (ws / "Gemfile.lock").write_text("GEM\n")
    (ws / "index.md").write_text("# hi\n")
    out.append(_workspace_case("synthetic__jekyll_gemfile", ws, ws))

    ws = base / "jekyll_bare" / "workspace"
    ws.mkdir(parents=True)
    (ws / "_config.yml").write_text("title: Test\ndestination: out\n")
    (ws / "index.md").write_text("# hi\n")
    (ws / "_redirects").write_text("/old /new 301\n")
    out.append(_workspace_case("synthetic__jekyll_bare", ws, ws))

    return out


def main() -> None:
    out_dir = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "target" / "rust-fixtures"
    out_dir.mkdir(parents=True, exist_ok=True)

    cases = []
    for example_dir in sorted(EXAMPLES.iterdir()):
        if not example_dir.is_dir():
            continue
        with _example_env(example_dir.name):
            case = _example_case(example_dir)
            if case:
                cases.append(case)
            cross = _example_case(example_dir, cross_platform="wasix_wasm32")
            if cross:
                cases.append(cross)
    cases.extend(_synthetic_cases(out_dir))

    manifest = {
        "starlib": str(ROOT / "src" / "shipit" / "starlib"),
        "snapshots": str(ROOT / "tests" / "plan_snapshots"),
        "cases": cases,
    }
    (out_dir / "manifest.json").write_text(json.dumps(manifest, indent=1))
    print(f"wrote {len(cases)} cases to {out_dir / 'manifest.json'}")


if __name__ == "__main__":
    main()
