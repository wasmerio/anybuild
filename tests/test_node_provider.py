from pathlib import Path

import pytest

from shipit.generator import load_provider
from shipit.providers.base import Config
from shipit.providers.laravel import LaravelProvider
from shipit.providers.node import NodeProvider, PackageManager
from shipit.providers.node_static import NodeStaticProvider


REPO_ROOT = Path(__file__).resolve().parents[1]


@pytest.mark.parametrize(
    ("lockfile", "package_manager"),
    [
        ("package-lock.json", PackageManager.NPM),
        ("pnpm-lock.yaml", PackageManager.PNPM),
        ("yarn.lock", PackageManager.YARN),
        ("bun.lockb", PackageManager.BUN),
    ],
)
def test_node_package_manager_lockfile_selection(
    tmp_path: Path, lockfile: str, package_manager: PackageManager
) -> None:
    (tmp_path / "package.json").write_text("{}\n")
    (tmp_path / lockfile).write_text("\n")

    provider_config = NodeProvider.load_config(tmp_path, Config())

    assert provider_config.package_manager == package_manager


def test_node_package_manager_defaults_to_npm(tmp_path: Path) -> None:
    (tmp_path / "package.json").write_text("{}\n")

    provider_config = NodeProvider.load_config(tmp_path, Config())

    assert provider_config.package_manager == PackageManager.NPM


def test_node_detection_does_not_beat_node_static() -> None:
    path = REPO_ROOT / "examples" / "vitepress"
    base_config = Config()

    node_static_result = NodeStaticProvider.detect(path, base_config)
    node_result = NodeProvider.detect(path, base_config)

    assert node_static_result is not None
    assert node_result is not None
    assert node_static_result.score > node_result.score
    assert load_provider(path, base_config) is NodeStaticProvider


def test_node_provider_detects_generic_node_example() -> None:
    path = REPO_ROOT / "examples" / "node"

    assert load_provider(path, Config()) is NodeProvider


def test_node_script_commands_prefers_build_by_default() -> None:
    package_json = {
        "scripts": {
            "generate": "node generate.js",
            "build": "node build.js",
        },
    }

    assert NodeProvider._script_commands(package_json) == [
        "node build.js",
        "node generate.js",
    ]


def test_node_start_command_prefers_explicit_command(tmp_path: Path) -> None:
    (tmp_path / "package.json").write_text(
        '{"scripts": {"start": "node server.js"}}\n'
    )
    base_config = Config()
    base_config.commands.start = "node custom.js"

    provider_config = NodeProvider.load_config(tmp_path, base_config)

    assert provider_config.commands.start == "node custom.js"


def test_node_start_command_uses_package_script(tmp_path: Path) -> None:
    (tmp_path / "package.json").write_text(
        '{"scripts": {"start": "node server.js"}}\n'
    )

    provider_config = NodeProvider.load_config(tmp_path, Config())

    assert provider_config.commands.start == "node server.js"


def test_node_start_command_uses_package_main(tmp_path: Path) -> None:
    (tmp_path / "package.json").write_text('{"main": "src/server.js"}\n')

    provider_config = NodeProvider.load_config(tmp_path, Config())

    assert provider_config.commands.start == "node src/server.js"


def test_node_start_command_uses_common_entry_file(tmp_path: Path) -> None:
    (tmp_path / "server.js").write_text("console.log('ok')\n")

    provider_config = NodeProvider.load_config(tmp_path, Config())

    assert provider_config.commands.start == "node server.js"


def test_laravel_reuses_node_provider_without_static_serving() -> None:
    path = REPO_ROOT / "examples" / "php-laravel-react"
    provider_config = LaravelProvider.load_config(path, Config())
    provider = LaravelProvider(path, provider_config)

    assert isinstance(provider.node_provider, NodeProvider)
    assert all("static_app" not in step for step in provider.build_steps())
    assert provider.commands()["start"].startswith('f"php ')
