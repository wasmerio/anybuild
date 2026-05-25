from pathlib import Path

import pytest
from pydantic import ValidationError

from shipit.generator import load_provider, load_provider_config
from shipit.providers.base import Config
from shipit.providers.node import NodeFramework, PackageManager
from shipit.providers.node_static import (
    NodeStaticConfig,
    NodeStaticProvider,
)


REPO_ROOT = Path(__file__).resolve().parents[1]


@pytest.mark.parametrize(
    ("example", "framework", "static_dir", "build_command"),
    [
        (
            "nodestatic-astro",
            NodeFramework.ASTRO,
            "dist",
            "npm run build",
        ),
        (
            "nodestatic-eleventy",
            NodeFramework.ELEVENTY,
            "_site",
            "npm run build",
        ),
        (
            "nodestatic-vitepress",
            NodeFramework.VITEPRESS,
            "docs/.vitepress/dist",
            "npm run docs:build",
        ),
        (
            "nodestatic-vuepress",
            NodeFramework.VUEPRESS,
            "docs/.vuepress/dist",
            "npm run docs:build",
        ),
        (
            "nodestatic-hexo",
            NodeFramework.HEXO,
            "public",
            "npm run generate",
        ),
        (
            "nodestatic-metalsmith",
            NodeFramework.METALSMITH,
            "build",
            "npm run build",
        ),
        (
            "nodestatic-assemble",
            NodeFramework.ASSEMBLE,
            "dist",
            "npm run build",
        ),
        (
            "nodestatic-harp",
            NodeFramework.HARP,
            "www",
            "npm run build",
        ),
    ],
)
def test_new_static_builder_examples_are_pure_static(
    example: str,
    framework: NodeFramework,
    static_dir: str,
    build_command: str,
) -> None:
    path = REPO_ROOT / "examples" / example
    base_config = Config()

    detect_result = NodeStaticProvider.detect(path, base_config)

    assert detect_result is not None
    assert detect_result.score == 60
    assert load_provider(path, base_config) is NodeStaticProvider

    provider_config = NodeStaticProvider.load_config(path, base_config)
    assert provider_config.framework == framework
    assert provider_config.static_dir == static_dir
    assert provider_config.build_command == build_command


def test_pure_static_dependency_keeps_priority_with_package_script_command() -> None:
    path = REPO_ROOT / "examples" / "nodestatic-vitepress"
    base_config = Config()
    base_config.commands.build = "npm run docs:build"

    detect_result = NodeStaticProvider.detect(path, base_config)

    assert detect_result is not None
    assert detect_result.score == 60


def test_explicit_next_build_command_uses_node_provider(tmp_path: Path) -> None:
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
    base_config = Config()
    base_config.commands.build = "next build"

    detect_result = NodeStaticProvider.detect(tmp_path, base_config)

    assert detect_result is not None
    assert detect_result.score == 20
    assert load_provider(tmp_path, base_config) is not NodeStaticProvider


def test_explicit_next_export_command_stays_node_static(tmp_path: Path) -> None:
    (tmp_path / "package.json").write_text(
        """{
  "scripts": {
    "build": "next export"
  },
  "dependencies": {
    "next": "^14.2.14"
  }
}
"""
    )
    base_config = Config()
    base_config.commands.build = "next export"

    detect_result = NodeStaticProvider.detect(tmp_path, base_config)

    assert detect_result is not None
    assert detect_result.score == 60
    assert load_provider(tmp_path, base_config) is NodeStaticProvider


def test_node_static_defaults_to_npm_without_lockfile(tmp_path: Path) -> None:
    (tmp_path / "package.json").write_text(
        """{
  "scripts": {
    "build": "vitepress build docs"
  },
  "dependencies": {
    "vitepress": "^1.6.4"
  }
}
"""
    )

    provider_config = NodeStaticProvider.load_config(tmp_path, Config())

    assert provider_config.package_manager == PackageManager.NPM
    assert provider_config.build_command == "npm run build"


def test_node_static_uses_pnpm_when_lockfile_is_present(tmp_path: Path) -> None:
    (tmp_path / "package.json").write_text(
        """{
  "scripts": {
    "build": "vitepress build docs"
  },
  "dependencies": {
    "vitepress": "^1.6.4"
  }
}
"""
    )
    (tmp_path / "pnpm-lock.yaml").write_text("lockfileVersion: '9.0'\n")

    provider_config = NodeStaticProvider.load_config(tmp_path, Config())

    assert provider_config.package_manager == PackageManager.PNPM
    assert provider_config.build_command == "pnpm run build"


def test_node_static_script_commands_prefers_build_over_fallbacks() -> None:
    package_json = {
        "scripts": {
            "build": "vite build",
            "generate": "vite generate",
            "docs:build": "vitepress build docs",
        },
    }

    assert NodeStaticProvider._script_commands(package_json) == ["vite build"]


@pytest.mark.parametrize(
    ("command", "framework"),
    [
        ("npx @11ty/eleventy", NodeFramework.ELEVENTY),
        ("vitepress build docs", NodeFramework.VITEPRESS),
        ("vuepress build docs", NodeFramework.VUEPRESS),
        ("hexo g", NodeFramework.HEXO),
        ("metalsmith", NodeFramework.METALSMITH),
        ("grunt assemble", NodeFramework.ASSEMBLE),
        ("harp compile src www", NodeFramework.HARP),
    ],
)
def test_new_static_builder_commands_are_detected(
    command: str, framework: NodeFramework
) -> None:
    assert NodeFramework.detect_from_command(command) == [framework]


def test_node_framework_static_capability_is_explicit() -> None:
    assert NodeFramework.NEXT.can_be_static()
    assert NodeFramework.ELEVENTY.can_be_static()
    assert not NodeFramework.EXPRESS.can_be_static()


def test_node_static_rejects_non_static_framework_config() -> None:
    with pytest.raises(ValidationError, match="express cannot be generated"):
        NodeStaticConfig(framework=NodeFramework.EXPRESS)


def test_node_static_rejects_non_static_framework_config_override(
    tmp_path: Path,
) -> None:
    (tmp_path / "package.json").write_text("{}\n")

    with pytest.raises(ValidationError, match="express cannot be generated"):
        load_provider_config(
            NodeStaticProvider,
            tmp_path,
            Config(),
            config={"framework": "express"},
        )
