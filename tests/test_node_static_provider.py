import json
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
            "nodestatic-gatsby",
            NodeFramework.GATSBY,
            "public",
            "npm run build",
        ),
        (
            "nodestatic-next",
            NodeFramework.NEXT,
            "out",
            "npm run build",
        ),
        (
            "nodestatic-nuxt",
            NodeFramework.NUXT_V3,
            ".output/public",
            "npm run generate",
        ),
        (
            "nodestatic-docusaurus",
            NodeFramework.DOCUSAURUS,
            "build",
            "npm run build",
        ),
        (
            "nodestatic-svelte",
            NodeFramework.SVELTEKIT,
            "build",
            "npm run build",
        ),
        (
            "nodestatic-sveltekit",
            NodeFramework.SVELTEKIT,
            "build",
            "npm run build",
        ),
        (
            "nodestatic-remix",
            NodeFramework.REMIX_V2_CLASSIC,
            "public",
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
        (
            "nodestatic-angular",
            NodeFramework.ANGULAR,
            "dist/angular-test",
            "npm run build",
        ),
        (
            "nodestatic-brunch",
            NodeFramework.BRUNCH,
            "public",
            "npm run build",
        ),
        (
            "nodestatic-create-react-app",
            NodeFramework.CREATE_REACT_APP,
            "build",
            "npm run build",
        ),
        (
            "nodestatic-docusaurus-old",
            NodeFramework.DOCUSAURUS_OLD,
            "build",
            "npm run build",
        ),
        (
            "nodestatic-ember",
            NodeFramework.EMBER,
            "dist",
            "npm run build",
        ),
        (
            "nodestatic-ionic-angular",
            NodeFramework.IONIC_ANGULAR,
            "www",
            "npm run build",
        ),
        (
            "nodestatic-ionic-react",
            NodeFramework.IONIC_REACT,
            "dist",
            "npm run build",
        ),
        (
            "nodestatic-parcel",
            NodeFramework.PARCEL,
            "dist",
            "npm run build",
        ),
        (
            "nodestatic-polymer",
            NodeFramework.POLYMER,
            "build/default",
            "npm run build",
        ),
        (
            "nodestatic-preact",
            NodeFramework.PREACT,
            "build",
            "npm run build",
        ),
        (
            "nodestatic-stencil",
            NodeFramework.STENCIL,
            "www",
            "npm run build",
        ),
        (
            "nodestatic-umijs",
            NodeFramework.UMIJS,
            "dist",
            "npm run build",
        ),
        (
            "nodestatic-vite",
            NodeFramework.VITE,
            "dist",
            "npm run build",
        ),
        (
            "nodestatic-vite-react",
            NodeFramework.VITE,
            "dist",
            "npm run build",
        ),
        (
            "nodestatic-vue",
            NodeFramework.VUE,
            "dist",
            "npm run build",
        ),
        (
            "nodestatic-sanity",
            NodeFramework.SANITY_V3,
            "dist",
            "npm run build",
        ),
        (
            "nodestatic-storybook",
            NodeFramework.STORYBOOK,
            "storybook-static",
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


def test_next_output_export_config_uses_node_static(tmp_path: Path) -> None:
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
    (tmp_path / "next.config.mjs").write_text(
        """const nextConfig = {
  output: "export",
};

export default nextConfig;
"""
    )

    provider_config = NodeStaticProvider.load_config(tmp_path, Config())

    assert load_provider(tmp_path, Config()) is NodeStaticProvider
    assert provider_config.framework == NodeFramework.NEXT
    assert provider_config.static_dir == "out"
    assert provider_config.build_command == "npm run build"


def test_elysia_dependency_uses_node_provider(tmp_path: Path) -> None:
    (tmp_path / "package.json").write_text(
        """{
  "scripts": {
    "build": "vite build",
    "start": "node server.js"
  },
  "dependencies": {
    "@elysia/node": "^1.4.6",
    "elysia": "^1.4.28",
    "vite": "^7.2.4"
  }
}
"""
    )

    assert NodeStaticProvider.detect(tmp_path, Config()) is None
    assert load_provider(tmp_path, Config()) is not NodeStaticProvider


def test_nuxt_generate_fallback_uses_node_static(tmp_path: Path) -> None:
    (tmp_path / "package.json").write_text(
        """{
  "scripts": {
    "build": "nuxt build",
    "generate": "nuxt generate"
  },
  "dependencies": {
    "nuxt": "^3.8.1"
  }
}
"""
    )

    provider_config = NodeStaticProvider.load_config(tmp_path, Config())

    assert load_provider(tmp_path, Config()) is NodeStaticProvider
    assert provider_config.framework == NodeFramework.NUXT_V3
    assert provider_config.static_dir == ".output/public"
    assert provider_config.build_command == "npm run generate"


def test_static_remix_output_can_use_node_static_with_node_dep(
    tmp_path: Path,
) -> None:
    (tmp_path / "public").mkdir()
    (tmp_path / "public" / "index.html").write_text("Remix static\n")
    (tmp_path / "package.json").write_text(
        """{
  "scripts": {
    "build": "remix build",
    "start": "serve -l 3000 public"
  },
  "dependencies": {
    "@remix-run/node": "^2.2.0"
  },
  "devDependencies": {
    "@remix-run/dev": "^2.2.0"
  }
}
"""
    )

    provider_config = NodeStaticProvider.load_config(tmp_path, Config())

    assert load_provider(tmp_path, Config()) is NodeStaticProvider
    assert provider_config.framework == NodeFramework.REMIX_V2_CLASSIC
    assert provider_config.static_dir == "public"
    assert provider_config.build_command == "npm run build"


@pytest.mark.parametrize(
    "dependencies",
    [
        {"@react-router/dev": "^7.1.5", "vite": "^5.0.0"},
        {"@remix-run/node": "^2.10.0", "@remix-run/dev": "^2.10.0"},
        {"@tanstack/react-start": "^1.0.0", "vite": "^5.0.0"},
        {"@solidjs/start": "^1.0.0", "vite": "^5.0.0"},
        {"@sveltejs/adapter-node": "^5.0.0", "@sveltejs/kit": "^2.16.1"},
        {"nitropack": "^2.11.0", "vite": "^5.0.0"},
        {"@shopify/hydrogen": "^2026.4.2", "vite": "^7.0.0"},
    ],
)
def test_runtime_vite_like_frameworks_are_not_node_static(
    tmp_path: Path,
    dependencies: dict[str, str],
) -> None:
    package_json = {
        "scripts": {
            "build": "vite build",
            "start": "node server.js",
        },
        "dependencies": dependencies,
    }
    (tmp_path / "package.json").write_text(json.dumps(package_json) + "\n")
    (tmp_path / "server.js").write_text("console.log('ok')\n")

    assert NodeStaticProvider.detect(tmp_path, Config()) is None
    assert load_provider(tmp_path, Config()) is not NodeStaticProvider


def test_hydrogen_config_is_not_node_static(tmp_path: Path) -> None:
    (tmp_path / "package.json").write_text(
        """{
  "scripts": {
    "build": "vite build",
    "start": "node server.js"
  },
  "dependencies": {
    "vite": "^7.0.0"
  }
}
"""
    )
    (tmp_path / "hydrogen.config.js").write_text("export default {}\n")
    (tmp_path / "server.js").write_text("console.log('ok')\n")

    assert NodeStaticProvider.detect(tmp_path, Config()) is None
    assert load_provider(tmp_path, Config()) is not NodeStaticProvider


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
    ("command", "frameworks"),
    [
        ("npx @11ty/eleventy", [NodeFramework.ELEVENTY]),
        ("vitepress build docs", [NodeFramework.VITEPRESS]),
        ("vuepress build docs", [NodeFramework.VUEPRESS]),
        ("hexo g", [NodeFramework.HEXO]),
        ("metalsmith", [NodeFramework.METALSMITH]),
        ("grunt assemble", [NodeFramework.ASSEMBLE]),
        ("harp compile src www", [NodeFramework.HARP]),
        ("ng build", [NodeFramework.IONIC_ANGULAR, NodeFramework.ANGULAR]),
        ("brunch build --production", [NodeFramework.BRUNCH]),
        (
            "react-scripts build",
            [NodeFramework.IONIC_REACT, NodeFramework.CREATE_REACT_APP],
        ),
        ("ember build --environment=production", [NodeFramework.EMBER]),
        ("parcel build src/index.html", [NodeFramework.PARCEL]),
        ("polymer build", [NodeFramework.POLYMER]),
        ("preact build", [NodeFramework.PREACT]),
        ("stencil build", [NodeFramework.STENCIL]),
        ("svelte-kit build", [NodeFramework.SVELTEKIT]),
        ("umi build", [NodeFramework.UMIJS]),
        ("vue-cli-service build", [NodeFramework.VUE]),
        ("nuxt generate", [NodeFramework.NUXT_OLD]),
        ("sanity build", [NodeFramework.SANITY]),
        ("storybook build", [NodeFramework.STORYBOOK]),
    ],
)
def test_new_static_builder_commands_are_detected(
    command: str, frameworks: list[NodeFramework]
) -> None:
    assert NodeFramework.detect_from_command(command) == frameworks


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
