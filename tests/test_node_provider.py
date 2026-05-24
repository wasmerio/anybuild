from pathlib import Path

import pytest

from shipit.generator import load_provider
from shipit.providers.base import Config
from shipit.providers.laravel import LaravelProvider
from shipit.providers.node import NodeFramework, NodeProvider, PackageManager
from shipit.providers.node_static import NodeStaticProvider
from shipit.providers.php import PhpFramework


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


def test_node_provider_detects_nextjs_runtime_app(tmp_path: Path) -> None:
    (tmp_path / "package.json").write_text(
        """{
  "scripts": {
    "build": "next build",
    "start": "next start"
  },
  "dependencies": {
    "next": "^14.2.14",
    "react": "^18.3.1",
    "react-dom": "^18.3.1"
  }
}
"""
    )

    provider_config = NodeProvider.load_config(tmp_path, Config())

    assert load_provider(tmp_path, Config()) is NodeProvider
    assert provider_config.framework == NodeFramework.NEXT
    assert provider_config.build_command == (
        "npx -y next-bundle --build-command 'npm run build'"
    )
    assert provider_config.commands.start == "node .next-bundle/server.mjs"


@pytest.mark.parametrize(
    ("lockfile", "build_command"),
    [
        (
            "package-lock.json",
            "npx -y next-bundle --build-command 'npm run build'",
        ),
        (
            "pnpm-lock.yaml",
            "pnpm dlx next-bundle --build-command 'pnpm run build'",
        ),
        (
            "yarn.lock",
            "yarn dlx next-bundle --build-command 'yarn run build'",
        ),
        (
            "bun.lockb",
            "bunx next-bundle --build-command 'bun run build'",
        ),
    ],
)
def test_nextjs_build_command_uses_package_manager(
    tmp_path: Path, lockfile: str, build_command: str
) -> None:
    (tmp_path / "package.json").write_text(
        """{
  "scripts": {
    "build": "next build"
  },
  "dependencies": {
    "next": "^14.2.14"
  }
}
"""
    )
    (tmp_path / lockfile).write_text("\n")

    provider_config = NodeProvider.load_config(tmp_path, Config())

    assert provider_config.build_command == build_command


def test_nextjs_build_command_wraps_explicit_build_command(
    tmp_path: Path,
) -> None:
    (tmp_path / "package.json").write_text(
        """{
  "dependencies": {
    "next": "^14.2.14"
  }
}
"""
    )
    base_config = Config()
    base_config.commands.build = "next build --debug"

    provider_config = NodeProvider.load_config(tmp_path, base_config)

    assert provider_config.build_command == (
        "npx -y next-bundle --build-command 'next build --debug'"
    )
    assert provider_config.commands.build == provider_config.build_command


def test_nextjs_start_command_prefers_explicit_command(tmp_path: Path) -> None:
    (tmp_path / "package.json").write_text(
        """{
  "scripts": {
    "build": "next build",
    "start": "next start"
  },
  "dependencies": {
    "next": "^14.2.14"
  }
}
"""
    )
    base_config = Config()
    base_config.commands.start = "node custom-next-server.js"

    provider_config = NodeProvider.load_config(tmp_path, base_config)

    assert provider_config.commands.start == "node custom-next-server.js"


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

    assert isinstance(provider, NodeProvider)
    assert provider_config.framework == PhpFramework.Laravel
    assert all("static_app" not in step for step in provider.build_steps())
    assert provider.commands()["start"].startswith('f"php ')
