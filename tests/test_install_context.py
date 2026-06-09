import json
from pathlib import Path

from shipit.providers.base import Config
from shipit.providers.install_context import (
    discover_js_install_context,
    discover_python_install_context,
)
from shipit.providers.node import NodeProvider, PackageManager
from shipit.providers.python import PythonProvider


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

    config = PythonProvider.load_config(tmp_path, Config())
    provider = PythonProvider(tmp_path, config)

    assert (
        'run("uv add -r requirements.txt uvicorn", '
        'inputs=["requirements.txt", "deps/base.txt"], group="install")'
    ) in provider.build_steps()


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
    provider = NodeProvider(app_dir, config)

    assert config.install_requires_all_files is True
    assert provider.build_steps_install() == [
        'copy(".", ".", ignore=["node_modules", ".git"])',
        'env(CI="true", NPM_CONFIG_FUND="false")',
        'run("npm install", group="install")',
    ]


def test_node_provider_keeps_narrow_install_without_local_deps(
    tmp_path: Path,
) -> None:
    (tmp_path / "package.json").write_text("{}\n")
    (tmp_path / "pnpm-lock.yaml").write_text("lockfileVersion: '9.0'\n")
    config = NodeProvider.load_config(tmp_path, Config())
    provider = NodeProvider(tmp_path, config)

    assert config.package_manager == PackageManager.PNPM
    assert provider.build_steps_install() == [
        'copy("pnpm-lock.yaml")',
        (
            'env(pnpm_config_minimum_release_age="0", '
            'CI="true", '
            'pnpm_config_dangerously_allow_all_builds="true")'
        ),
        'run("pnpm install", inputs=["package.json"], group="install")',
    ]


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
