import json
import shutil
import subprocess
from pathlib import Path

import pytest

from shipit.generator import load_provider
from shipit.providers.base import Config
from shipit.shipit_types import CopyStep, RunStep

from tests.plan_helpers import evaluate_project_plan
from shipit.providers.laravel import LaravelProvider
from shipit.providers.node import (
    NodeConfig,
    NodeFramework,
    NodeProvider,
    PackageManager,
)
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


def test_node_package_manager_uses_package_manager_field(tmp_path: Path) -> None:
    (tmp_path / "package.json").write_text(
        '{"packageManager": "pnpm@10.0.0"}\n'
    )
    (tmp_path / "package-lock.json").write_text("{}\n")

    provider_config = NodeProvider.load_config(tmp_path, Config())

    assert provider_config.package_manager == PackageManager.PNPM


def test_node_package_manager_uses_pnpm_workspace(tmp_path: Path) -> None:
    (tmp_path / "package.json").write_text("{}\n")
    (tmp_path / "package-lock.json").write_text("{}\n")
    (tmp_path / "pnpm-workspace.yaml").write_text("packages:\n  - apps/*\n")

    provider_config = NodeProvider.load_config(tmp_path, Config())

    assert provider_config.package_manager == PackageManager.PNPM


def test_node_check_deps_returns_matching_dependencies(tmp_path: Path) -> None:
    (tmp_path / "package.json").write_text(
        """{
  "dependencies": {
    "express": "5.1.0"
  },
  "devDependencies": {
    "hono": "^4.12.23"
  }
}
"""
    )

    found_deps = NodeProvider.check_deps(
        tmp_path,
        "express",
        "elysia",
        "hono",
    )

    assert found_deps == {"express", "hono"}


def test_node_detection_does_not_beat_node_static() -> None:
    path = REPO_ROOT / "examples" / "nodestatic-vitepress"
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


def test_common_entry_without_package_json_needs_node_evidence(
    tmp_path: Path,
) -> None:
    (tmp_path / "app.js").write_text(
        """import http from "node:http";

http.createServer((_req, res) => {
  res.end("ok");
}).listen(process.env.PORT || 8080);
"""
    )

    provider_config = NodeProvider.load_config(tmp_path, Config())

    assert load_provider(tmp_path, Config()) is NodeProvider
    assert provider_config.commands.start == "node app.js"


@pytest.mark.parametrize(
    ("example", "framework"),
    [
        ("node-fastify", NodeFramework.FASTIFY),
        ("node-hono", NodeFramework.HONO),
        ("node-express", NodeFramework.EXPRESS),
        ("node-koa", NodeFramework.KOA),
        ("node-h3", NodeFramework.H3),
        ("node-elysia", NodeFramework.ELYSIA),
        ("node-nestjs", NodeFramework.NESTJS),
        ("node-nitro", NodeFramework.NITRO),
        ("node-hydrogen", NodeFramework.HYDROGEN),
        ("node-react-router", NodeFramework.REACT_ROUTER),
        ("node-remix", NodeFramework.REMIX),
        ("node-solidstart", NodeFramework.SOLIDSTART),
        ("node-tanstack-start", NodeFramework.TANSTACK_START),
        ("node-xmcp", NodeFramework.XMCP),
        ("node-mastra", NodeFramework.MASTRA),
    ],
)
def test_node_provider_detects_node_runtime_examples(
    example: str, framework: NodeFramework
) -> None:
    path = REPO_ROOT / "examples" / example

    assert load_provider(path, Config()) is NodeProvider
    provider_config = NodeProvider.load_config(path, Config())
    assert provider_config.framework == framework
    assert provider_config.commands.start == "node server.js"


def test_node_provider_detects_astro_runtime_example() -> None:
    path = REPO_ROOT / "examples" / "node-astro"

    assert load_provider(path, Config()) is NodeProvider


def test_node_provider_detects_hydrogen_config_file(tmp_path: Path) -> None:
    (tmp_path / "package.json").write_text("{}\n")
    (tmp_path / "hydrogen.config.ts").write_text("export default {}\n")

    provider_config = NodeProvider.load_config(tmp_path, Config())

    assert load_provider(tmp_path, Config()) is NodeProvider
    assert provider_config.framework == NodeFramework.HYDROGEN


def test_node_provider_detects_elysia_runtime_example() -> None:
    path = REPO_ROOT / "examples" / "node-elysia"

    assert load_provider(path, Config()) is NodeProvider
    provider_config = NodeProvider.load_config(path, Config())
    assert provider_config.framework == NodeFramework.ELYSIA
    assert provider_config.commands.start == "node server.js"


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
        "npx -y next-bundle@1.0.0 --build-command 'npm run build'"
    )
    assert provider_config.commands.start == "node server.mjs"


@pytest.mark.parametrize(
    ("lockfile", "build_command"),
    [
        (
            "package-lock.json",
            "npx -y next-bundle@1.0.0 --build-command 'npm run build'",
        ),
        (
            "pnpm-lock.yaml",
            "pnpm dlx next-bundle@1.0.0 --build-command 'pnpm run build'",
        ),
        (
            "yarn.lock",
            "yarn dlx next-bundle@1.0.0 --build-command 'yarn run build'",
        ),
        (
            "bun.lockb",
            "bunx next-bundle@1.0.0 --build-command 'bun run build'",
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
        "npx -y next-bundle@1.0.0 --build-command 'next build --debug'"
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


def test_node_script_commands_uses_build_by_default() -> None:
    package_json = {
        "scripts": {
            "generate": "node generate.js",
            "build": "node build.js",
        },
    }

    assert NodeProvider._script_commands(package_json) == ["node build.js"]


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
    (tmp_path / "server.js").write_text(
        """const http = require("http");

http.createServer((_req, res) => {
  res.end("ok");
}).listen(8080);
"""
    )

    provider_config = NodeProvider.load_config(tmp_path, Config())

    assert provider_config.commands.start == "node server.js"


def test_node_build_steps_optimize_deps_prunes_then_node_modules(
    tmp_path: Path,
) -> None:
    (tmp_path / "package.json").write_text("{}\n")
    (tmp_path / "package-lock.json").write_text("{}\n")

    backend, _ctx, serve, _config = evaluate_project_plan(
        tmp_path, tmp_path, config_overrides={"remove_native_binaries": True}
    )
    steps = serve.build
    app_path = backend.get_build_mount_path("app")
    assets_path = backend.get_build_mount_path("assets")

    # The app is exported last, after the node_modules optimizations.
    assert isinstance(steps[-1], RunStep)
    assert steps[-1].command == f"cp -R . {app_path}"

    def index_of(predicate):
        return next(i for i, step in enumerate(steps) if predicate(step))

    prune_index = index_of(
        lambda s: isinstance(s, RunStep) and s.group == "prune"
    )
    mkdir_index = index_of(
        lambda s: isinstance(s, RunStep)
        and s.group == "optimize"
        and s.command == f"mkdir -p {assets_path}"
    )
    copy_index = index_of(
        lambda s: isinstance(s, CopyStep)
        and s.source == "node/optimize-node-modules.sh"
        and s.base == "assets"
    )
    optimize_index = index_of(
        lambda s: isinstance(s, RunStep)
        and s.command == f"bash {assets_path}/optimize-node-modules.sh node_modules"
    )
    assert prune_index < mkdir_index < copy_index < optimize_index


def test_node_provider_skips_native_binary_optimizer_by_default(
    tmp_path: Path,
) -> None:
    (tmp_path / "package.json").write_text("{}\n")

    _backend, ctx, serve, config = evaluate_project_plan(tmp_path, tmp_path)
    assert config.remove_native_binaries is False
    assert all(
        "optimize-node-modules.sh" not in getattr(step, "command", "")
        for step in serve.build
        if isinstance(step, RunStep)
    )
    assert "bash" not in ctx.packages
    assert all(mount.name != "assets" for mount in ctx.mounts)


def test_node_prepare_steps_use_precompile_edgejs_flag(tmp_path: Path) -> None:
    (tmp_path / "package.json").write_text(
        json.dumps({"scripts": {"start": "node server.js"}})
    )
    (tmp_path / "server.js").write_text("console.log('ok')\n")

    _b, _ctx, serve, config = evaluate_project_plan(tmp_path, tmp_path / "default")
    assert config.precompile_edgejs is None
    assert not serve.prepare

    _b, _ctx, serve, _config = evaluate_project_plan(
        tmp_path,
        tmp_path / "precompile",
        config_overrides={"precompile_edgejs": True},
    )
    assert serve.prepare is not None
    assert [step.command for step in serve.prepare] == [
        f"edgejs --precompile {serve.cwd}"
    ]


def test_node_provider_uses_build_only_assets_mount(tmp_path: Path) -> None:
    (tmp_path / "package.json").write_text("{}\n")

    _backend, ctx, serve, _config = evaluate_project_plan(
        tmp_path, tmp_path, config_overrides={"remove_native_binaries": True}
    )
    # The assets mount exists for the build but is not attached to the serve.
    assert any(mount.name == "assets" for mount in ctx.mounts)
    assert all(mount.name != "assets" for mount in (serve.mounts or []))


def test_node_modules_binary_optimizer_removes_executable_binaries(
    tmp_path: Path,
) -> None:
    bash = shutil.which("bash")
    if bash is None:
        pytest.skip("optimizer script requires bash")

    package_dir = tmp_path / "node_modules" / "package"
    package_dir.mkdir(parents=True)
    executable_script = package_dir / "script"
    executable_binary = package_dir / "native"
    executable_wasm = package_dir / "module.wasm"
    executable_wasm_magic = package_dir / "wasm-runtime"
    non_executable_binary = package_dir / "native-data"

    executable_script.write_text("#!/bin/sh\necho ok\n")
    executable_binary.write_bytes(b"\x7fELF\x00binary")
    executable_wasm.write_bytes(b"\x00asm\x01\x00\x00\x00")
    executable_wasm_magic.write_bytes(b"\x00asm\x01\x00\x00\x00")
    non_executable_binary.write_bytes(b"\x7fELF\x00binary")

    for path in (
        executable_script,
        executable_binary,
        executable_wasm,
        executable_wasm_magic,
    ):
        path.chmod(0o755)
    non_executable_binary.chmod(0o644)

    script = REPO_ROOT / "src" / "shipit" / "assets" / "node"
    script = script / "optimize-node-modules.sh"

    subprocess.run([bash, str(script), "node_modules"], cwd=tmp_path, check=True)

    assert executable_script.exists()
    assert not executable_binary.exists()
    assert executable_wasm.exists()
    assert executable_wasm_magic.exists()
    assert non_executable_binary.exists()


def test_laravel_reuses_node_provider_without_static_serving(
    tmp_path: Path,
) -> None:
    path = REPO_ROOT / "examples" / "php-laravel-react"
    provider_config = LaravelProvider.load_config(path, Config())
    assert provider_config.framework == PhpFramework.Laravel

    _backend, ctx, serve, _config = evaluate_project_plan(path, tmp_path)
    assert serve.provider == "laravel"
    assert all(mount.name != "static_app" for mount in ctx.mounts)
    assert serve.commands["start"].startswith("php -S localhost:")
