import json
from pathlib import Path

from shipit.providers.base import Config
from shipit.shipit_types import CopyStep, EnvStep, RunStep
from shipit.providers.install_context import (
    discover_js_install_context,
    discover_python_install_context,
)
from shipit.providers.node import NodeProvider, PackageManager
from shipit.providers.python import PythonProvider

from tests.plan_helpers import evaluate_project_plan


def test_python_requirements_context_follows_recursive_includes(
    tmp_path: Path,
) -> None:
    deps_dir = tmp_path / "deps"
    deps_dir.mkdir()
    (tmp_path / "requirements.txt").write_text(
        "-r deps/base.txt\nflask==3.0.0\n"
    )
    (deps_dir / "base.txt").write_text(
        "--constraint ../constraints.txt\nfastapi==0.115.0\n"
    )
    (tmp_path / "constraints.txt").write_text("anyio<5\n")

    context = discover_python_install_context(
        tmp_path,
        include_requirements=True,
    )

    assert context.inputs == [
        "requirements.txt",
        "deps/base.txt",
        "constraints.txt",
    ]
    assert context.requires_all_files is False


def test_python_requirements_context_detects_external_local_package(
    tmp_path: Path,
) -> None:
    app_dir = tmp_path / "app"
    shared_dir = tmp_path / "shared"
    app_dir.mkdir()
    shared_dir.mkdir()
    (app_dir / "requirements.txt").write_text("-e ../shared\n")
    (shared_dir / "pyproject.toml").write_text("[project]\nname = 'shared'\n")

    context = discover_python_install_context(
        app_dir,
        include_requirements=True,
    )

    assert context.requires_all_files is True
    assert shared_dir in context.local_paths


def test_python_provider_uses_context_for_requirements_inputs(
    tmp_path: Path,
) -> None:
    deps_dir = tmp_path / "deps"
    deps_dir.mkdir()
    (tmp_path / "requirements.txt").write_text("-r deps/base.txt\n")
    (deps_dir / "base.txt").write_text("fastapi==0.115.0\n")

    (tmp_path / "main.py").write_text("print('ok')\n")

    _backend, _ctx, serve, _config = evaluate_project_plan(
        tmp_path, tmp_path, use_provider="python"
    )
    install_step = next(
        step
        for step in serve.build
        if isinstance(step, RunStep)
        and step.command == "uv add -r requirements.txt uvicorn"
    )
    assert install_step.inputs == ["requirements.txt", "deps/base.txt"]


def test_python_pyproject_context_detects_uv_path_source(
    tmp_path: Path,
) -> None:
    package_dir = tmp_path / "packages" / "shared"
    package_dir.mkdir(parents=True)
    (package_dir / "pyproject.toml").write_text("[project]\nname = 'shared'\n")
    (tmp_path / "pyproject.toml").write_text(
        """
[project]
name = "app"
dependencies = ["shared"]

[tool.uv.sources]
shared = { path = "packages/shared" }
""".lstrip()
    )

    context = discover_python_install_context(
        tmp_path,
        include_pyproject=True,
    )

    assert context.requires_all_files is True
    assert package_dir in context.local_paths


def test_python_pyproject_context_ignores_remote_direct_url(
    tmp_path: Path,
) -> None:
    (tmp_path / "pyproject.toml").write_text(
        """
[project]
name = "app"
dependencies = ["shared @ https://example.com/shared.whl"]
""".lstrip()
    )

    context = discover_python_install_context(
        tmp_path,
        include_pyproject=True,
    )

    assert context.requires_all_files is False
    assert context.local_paths == []


def test_js_context_detects_recursive_external_file_dependency(
    tmp_path: Path,
) -> None:
    app_dir = tmp_path / "app"
    shared_dir = tmp_path / "shared"
    core_dir = tmp_path / "core"
    app_dir.mkdir()
    shared_dir.mkdir()
    core_dir.mkdir()
    (app_dir / "package.json").write_text(
        json.dumps(
            {
                "dependencies": {
                    "shared": "file:../shared",
                },
            }
        )
    )
    (shared_dir / "package.json").write_text(
        json.dumps(
            {
                "name": "shared",
                "dependencies": {
                    "core": "file:../core",
                },
            }
        )
    )
    (core_dir / "package.json").write_text(
        json.dumps({"name": "core"})
    )

    context = discover_js_install_context(app_dir)

    assert context.requires_all_files is True
    assert context.local_paths == [shared_dir, core_dir]


def test_js_context_detects_in_root_file_dependency(tmp_path: Path) -> None:
    package_dir = tmp_path / "packages" / "shared"
    package_dir.mkdir(parents=True)
    (tmp_path / "package.json").write_text(
        json.dumps({"dependencies": {"shared": "file:packages/shared"}})
    )
    (package_dir / "package.json").write_text(
        json.dumps({"name": "shared"})
    )

    context = discover_js_install_context(tmp_path)

    assert context.requires_all_files is True
    assert context.local_paths == [package_dir]


def test_node_provider_uses_escape_hatch_for_external_local_dependency(
    tmp_path: Path,
) -> None:
    app_dir = tmp_path / "app"
    shared_dir = tmp_path / "shared"
    app_dir.mkdir()
    shared_dir.mkdir()
    (app_dir / "package.json").write_text(
        json.dumps({"dependencies": {"shared": "file:../shared"}})
    )
    (shared_dir / "package.json").write_text(
        json.dumps({"name": "shared"})
    )

    config = NodeProvider.load_config(app_dir, Config())
    assert config.install_requires_all_files is True

    _backend, _ctx, serve, _config = evaluate_project_plan(
        app_dir, tmp_path, use_provider="node"
    )
    copy_step, env_step, install_step = serve.build[2:5]
    assert isinstance(copy_step, CopyStep)
    assert copy_step.source == "."
    assert copy_step.ignore == ["node_modules", ".git"]
    assert isinstance(env_step, EnvStep)
    assert env_step.variables == {"CI": "true", "NPM_CONFIG_FUND": "false"}
    assert isinstance(install_step, RunStep)
    assert install_step.command == "npm install"
    assert install_step.inputs is None


def test_node_provider_keeps_narrow_install_without_local_deps(
    tmp_path: Path,
) -> None:
    (tmp_path / "package.json").write_text("{}\n")
    (tmp_path / "pnpm-lock.yaml").write_text("lockfileVersion: '9.0'\n")
    config = NodeProvider.load_config(tmp_path, Config())
    assert config.package_manager == PackageManager.PNPM

    _backend, _ctx, serve, _config = evaluate_project_plan(
        tmp_path, tmp_path, use_provider="node"
    )
    copy_step, env_step, install_step = serve.build[2:5]
    assert isinstance(copy_step, CopyStep)
    assert copy_step.source == "pnpm-lock.yaml"
    assert isinstance(env_step, EnvStep)
    assert env_step.variables == {
        "pnpm_config_minimum_release_age": "0",
        "CI": "true",
        "pnpm_config_dangerously_allow_all_builds": "true",
    }
    assert isinstance(install_step, RunStep)
    assert install_step.command == "pnpm install"
    assert install_step.inputs == ["package.json"]


def test_js_context_detects_package_json_workspaces(tmp_path: Path) -> None:
    package_dir = tmp_path / "packages" / "ui"
    package_dir.mkdir(parents=True)
    (tmp_path / "package.json").write_text(
        json.dumps({"workspaces": ["packages/*"]})
    )
    (package_dir / "package.json").write_text(
        json.dumps({"name": "@app/ui"})
    )

    context = discover_js_install_context(tmp_path)

    assert context.requires_all_files is True
    assert package_dir in context.local_paths
