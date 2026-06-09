import json
import shlex
from enum import Enum
from pathlib import Path
from typing import Any, ClassVar, Dict, Optional, Set

import yaml
from pydantic import Field
from pydantic_settings import SettingsConfigDict
from semantic_version import NpmSpec, Version

from .base import (
    Config,
    DependencySpec,
    DetectResult,
    MountSpec,
    ServiceSpec,
    VolumeSpec,
    subdir_build_context_steps,
)
from .install_context import (
    discover_js_install_context,
    starlark_string_list,
)


NODE_MODULES_OPTIMIZER_ASSET = "node/optimize-node-modules.sh"
OPTIMIZE_DEPS_VERSION = "0.1.1"

class PackageManager(Enum):
    NPM = "npm"
    PNPM = "pnpm"
    YARN = "yarn"
    BUN = "bun"

    def prune_command(self) -> str:
        return {
            PackageManager.NPM: "npm prune --omit=dev --ignore-scripts",
            PackageManager.PNPM: "pnpm prune --prod",
            PackageManager.YARN: "yarn workspaces focus --all --production",
            PackageManager.BUN: "rm -rf node_modules && bun install --omit=dev --ignore-scripts",
        }[self]

    def as_dependency(self, path: Path) -> DependencySpec:
        dep_name = {
            PackageManager.NPM: "npm",
            PackageManager.PNPM: "pnpm",
            PackageManager.YARN: "yarn",
            PackageManager.BUN: "bun",
        }[self]

        default_version = None
        if self == PackageManager.PNPM:
            lockfile = path / self.lockfile()
            lockfile_version = self.pnpm_lockfile_version(lockfile)
            if lockfile_version:
                if lockfile_version.startswith("5."):
                    default_version = "7"
                elif lockfile_version.startswith("6."):
                    default_version = "8"

        return DependencySpec(
            dep_name,
            var_name=f"config.{dep_name.lower()}_version",
            default_version=default_version,
        )

    def lockfile(self) -> str:
        return {
            PackageManager.NPM: "package-lock.json",
            PackageManager.PNPM: "pnpm-lock.yaml",
            PackageManager.YARN: "yarn.lock",
            PackageManager.BUN: "bun.lockb",
        }[self]

    @classmethod
    def pnpm_lockfile_version(cls, lockfile: Path) -> Optional[str]:
        if not lockfile.exists():
            return None
        with open(lockfile, "r") as f:
            for line in f:
                if "lockfileVersion" in line:
                    try:
                        config = yaml.safe_load(line)
                        version = config.get("lockfileVersion")
                        if isinstance(version, bytes):
                            return version.decode()
                        if isinstance(version, str):
                            return version
                    except Exception:
                        pass
        return None

    def install_command(self, has_lockfile: bool = False) -> str:
        return {
            PackageManager.NPM: f"npm install",
            PackageManager.PNPM: "pnpm install",
            PackageManager.YARN: "yarn install",
            PackageManager.BUN: f"bun install{' --no-save' if has_lockfile else ''}",
        }[self]

    def run_command(self, command: str) -> str:
        return {
            PackageManager.NPM: f"npm run {command}",
            PackageManager.PNPM: f"pnpm run {command}",
            PackageManager.YARN: f"yarn run {command}",
            PackageManager.BUN: f"bun run {command}",
        }[self]

    def run_execute_command(self, command: str) -> str:
        return {
            PackageManager.NPM: f"npx {command}",
            PackageManager.PNPM: f"pnpx {command}",
            PackageManager.YARN: f"ypx {command}",
            PackageManager.BUN: f"bunx {command}",
        }[self]

    def dlx_command(self, command: str) -> str:
        return {
            PackageManager.NPM: f"npx -y {command}",
            PackageManager.PNPM: f"pnpm dlx {command}",
            PackageManager.YARN: f"yarn dlx {command}",
            PackageManager.BUN: f"bunx {command}",
        }[self]


class NodeFramework(Enum):
    ANGULAR = "angular"
    NEXT = "next"
    ASTRO = "astro"
    BRUNCH = "brunch"
    CREATE_REACT_APP = "create-react-app"
    VITE = "vite"
    GATSBY = "gatsby"
    ELEVENTY = "eleventy"
    EMBER = "ember"
    VITEPRESS = "vitepress"
    VUEPRESS = "vuepress"
    HEXO = "hexo"
    IONIC_ANGULAR = "ionic-angular"
    IONIC_REACT = "ionic-react"
    METALSMITH = "metalsmith"
    ASSEMBLE = "assemble"
    HARP = "harp"
    PARCEL = "parcel"
    POLYMER = "polymer"
    PREACT = "preact"
    STENCIL = "stencil"
    DOCUSAURUS_OLD = "docusaurus-old"
    DOCUSAURUS = "docusaurus"
    SVELTE = "svelte"
    SVELTEKIT = "sveltekit"
    UMIJS = "umijs"
    VUE = "vue"
    REMIX = "remix"
    NUXT_OLD = "nuxt"
    NUXT_V3 = "nuxt3"
    REMIX_OLD = "remix-old"
    REMIX_V2 = "remix-v2"
    REMIX_V2_CLASSIC = "remix-v2-classic"
    REACT_ROUTER = "react-router"
    NITRO = "nitro"
    SOLIDSTART = "solidstart"
    TANSTACK_START = "tanstack-start"
    HYDROGEN = "hydrogen"
    HONO = "hono"
    EXPRESS = "express"
    H3 = "h3"
    KOA = "koa"
    NESTJS = "nestjs"
    ELYSIA = "elysia"
    FASTIFY = "fastify"
    XMCP = "xmcp"
    MASTRA = "mastra"
    SANITY = "sanity"
    SANITY_V3 = "sanity-v3"
    STORYBOOK = "storybook"

    def can_be_static(self) -> bool:
        return self in {
            NodeFramework.ANGULAR,
            NodeFramework.ASTRO,
            NodeFramework.BRUNCH,
            NodeFramework.CREATE_REACT_APP,
            NodeFramework.EMBER,
            NodeFramework.VITE,
            NodeFramework.NEXT,
            NodeFramework.GATSBY,
            NodeFramework.ELEVENTY,
            NodeFramework.VITEPRESS,
            NodeFramework.VUEPRESS,
            NodeFramework.HEXO,
            NodeFramework.IONIC_ANGULAR,
            NodeFramework.IONIC_REACT,
            NodeFramework.METALSMITH,
            NodeFramework.ASSEMBLE,
            NodeFramework.HARP,
            NodeFramework.PARCEL,
            NodeFramework.POLYMER,
            NodeFramework.PREACT,
            NodeFramework.STENCIL,
            NodeFramework.DOCUSAURUS_OLD,
            NodeFramework.DOCUSAURUS,
            NodeFramework.SVELTE,
            NodeFramework.SVELTEKIT,
            NodeFramework.UMIJS,
            NodeFramework.VUE,
            NodeFramework.REMIX,
            NodeFramework.NUXT_OLD,
            NodeFramework.NUXT_V3,
            NodeFramework.REMIX_OLD,
            NodeFramework.REMIX_V2,
            NodeFramework.REMIX_V2_CLASSIC,
            NodeFramework.SANITY,
            NodeFramework.SANITY_V3,
            NodeFramework.STORYBOOK,
        }

    def is_pure_static(self) -> bool:
        return self in {
            NodeFramework.ANGULAR,
            NodeFramework.BRUNCH,
            NodeFramework.CREATE_REACT_APP,
            NodeFramework.ELEVENTY,
            NodeFramework.EMBER,
            NodeFramework.VITEPRESS,
            NodeFramework.VUEPRESS,
            NodeFramework.ASSEMBLE,
            NodeFramework.HARP,
            NodeFramework.HEXO,
            NodeFramework.IONIC_ANGULAR,
            NodeFramework.IONIC_REACT,
            NodeFramework.METALSMITH,
            NodeFramework.PARCEL,
            NodeFramework.POLYMER,
            NodeFramework.PREACT,
            NodeFramework.STENCIL,
            NodeFramework.DOCUSAURUS,
            NodeFramework.DOCUSAURUS_OLD,
            NodeFramework.SVELTE,
            NodeFramework.SVELTEKIT,
            NodeFramework.UMIJS,
            NodeFramework.VITE,
            NodeFramework.VUE,
            NodeFramework.SANITY,
            NodeFramework.SANITY_V3,
            NodeFramework.STORYBOOK,
        }

    def get_static_output_dir(self) -> str:
        output_dirs = {
            NodeFramework.ANGULAR: "dist",
            NodeFramework.NEXT: "out",
            NodeFramework.ELEVENTY: "_site",
            NodeFramework.NUXT_V3: ".output/public",
            NodeFramework.ASTRO: "dist",
            NodeFramework.BRUNCH: "public",
            NodeFramework.CREATE_REACT_APP: "build",
            NodeFramework.EMBER: "dist",
            NodeFramework.VITE: "dist",
            NodeFramework.NUXT_OLD: "dist",
            NodeFramework.GATSBY: "public",
            NodeFramework.HEXO: "public",
            NodeFramework.IONIC_ANGULAR: "www",
            NodeFramework.IONIC_REACT: "dist",
            NodeFramework.VITEPRESS: "docs/.vitepress/dist",
            NodeFramework.VUEPRESS: "docs/.vuepress/dist",
            NodeFramework.REMIX_OLD: "build/client",
            NodeFramework.REMIX: "build/client",
            NodeFramework.REMIX_V2: "build/client",
            NodeFramework.REMIX_V2_CLASSIC: "public",
            NodeFramework.ASSEMBLE: "dist",
            NodeFramework.HARP: "www",
            NodeFramework.PARCEL: "dist",
            NodeFramework.POLYMER: "build/default",
            NodeFramework.PREACT: "build",
            NodeFramework.STENCIL: "www",
            NodeFramework.DOCUSAURUS: "build",
            NodeFramework.DOCUSAURUS_OLD: "build",
            NodeFramework.SVELTE: "build",
            NodeFramework.SVELTEKIT: "build",
            NodeFramework.UMIJS: "dist",
            NodeFramework.VUE: "dist",
            NodeFramework.METALSMITH: "build",
            NodeFramework.SANITY: "dist",
            NodeFramework.SANITY_V3: "dist",
            NodeFramework.STORYBOOK: "storybook-static",
        }
        if self not in output_dirs:
            raise ValueError(
                f"{self.value} cannot be generated as a static Node app"
            )
        return output_dirs[self]

    @classmethod
    def detect_from_command(cls, build_command: str) -> list["NodeFramework"]:
        commands = {
            "ng": [NodeFramework.IONIC_ANGULAR, NodeFramework.ANGULAR],
            "gatsby": [NodeFramework.GATSBY],
            "astro": [NodeFramework.ASTRO],
            "@11ty/eleventy": [NodeFramework.ELEVENTY],
            "eleventy": [NodeFramework.ELEVENTY],
            "brunch": [NodeFramework.BRUNCH],
            "react-scripts": [
                NodeFramework.IONIC_REACT,
                NodeFramework.CREATE_REACT_APP,
            ],
            "remix-ssg": [NodeFramework.REMIX_OLD],
            "remix": [NodeFramework.REMIX_V2_CLASSIC, NodeFramework.REMIX_V2],
            "ember": [NodeFramework.EMBER],
            "vite": [NodeFramework.VITE],
            "vitepress": [NodeFramework.VITEPRESS],
            "vuepress": [NodeFramework.VUEPRESS],
            "hexo": [NodeFramework.HEXO],
            "metalsmith": [NodeFramework.METALSMITH],
            "harp": [NodeFramework.HARP],
            "parcel": [NodeFramework.PARCEL],
            "polymer": [NodeFramework.POLYMER],
            "preact": [NodeFramework.PREACT],
            "stencil": [NodeFramework.STENCIL],
            "docusaurus": [
                NodeFramework.DOCUSAURUS,
                NodeFramework.DOCUSAURUS_OLD,
            ],
            "next": [NodeFramework.NEXT],
            "nuxi": [NodeFramework.NUXT_V3],
            "nuxt": [NodeFramework.NUXT_OLD],
            "svelte-kit": [NodeFramework.SVELTEKIT],
            "umi": [NodeFramework.UMIJS],
            "vue-cli-service": [NodeFramework.VUE],
            "sanity": [NodeFramework.SANITY],
            "storybook": [NodeFramework.STORYBOOK],
        }
        try:
            tokens = shlex.split(build_command)
        except ValueError:
            tokens = build_command.split()

        for index, token in enumerate(tokens):
            if token == "grunt" and "assemble" in tokens[index + 1 :]:
                return [NodeFramework.ASSEMBLE]
            if token in commands:
                return commands[token]
        return []

    def build_static_command(self) -> str:
        build_commands = {
            NodeFramework.ANGULAR: "ng build",
            NodeFramework.GATSBY: "gatsby build",
            NodeFramework.ELEVENTY: "@11ty/eleventy",
            NodeFramework.VITEPRESS: "vitepress build docs",
            NodeFramework.VUEPRESS: "vuepress build docs",
            NodeFramework.HEXO: "hexo generate",
            NodeFramework.METALSMITH: "metalsmith build",
            NodeFramework.ASSEMBLE: "grunt assemble",
            NodeFramework.HARP: "harp compile . www",
            NodeFramework.BRUNCH: "brunch build --production",
            NodeFramework.CREATE_REACT_APP: "react-scripts build",
            NodeFramework.EMBER: "ember build",
            NodeFramework.IONIC_ANGULAR: "ng build",
            NodeFramework.IONIC_REACT: "vite build",
            NodeFramework.PARCEL: "parcel build",
            NodeFramework.POLYMER: "polymer build",
            NodeFramework.PREACT: "preact build",
            NodeFramework.STENCIL: "stencil build",
            NodeFramework.ASTRO: "astro build",
            NodeFramework.REMIX_OLD: "remix-ssg build",
            NodeFramework.REMIX_V2: "vite build",
            NodeFramework.REMIX_V2_CLASSIC: "remix build",
            NodeFramework.DOCUSAURUS: "docusaurus build",
            NodeFramework.DOCUSAURUS_OLD: "docusaurus build",
            NodeFramework.SVELTE: "svelte-kit build",
            NodeFramework.SVELTEKIT: "svelte-kit build",
            NodeFramework.UMIJS: "umi build",
            NodeFramework.VITE: "vite build",
            NodeFramework.VUE: "vue-cli-service build",
            NodeFramework.NEXT: "next export",
            NodeFramework.NUXT_V3: "nuxi generate",
            NodeFramework.NUXT_OLD: "nuxt generate",
            NodeFramework.REMIX: "remix build",
            NodeFramework.SANITY: "sanity build",
            NodeFramework.SANITY_V3: "sanity build",
            NodeFramework.STORYBOOK: "storybook build",
        }
        if self not in build_commands:
            raise ValueError(
                f"{self.value} cannot be generated as a static Node app"
            )
        return build_commands[self]

    def bundle_build_command(
        self, package_manager: PackageManager, build_command: str
    ) -> str:
        if self == NodeFramework.NEXT:
            quoted_command = shlex.quote(build_command)
            return package_manager.dlx_command(
                f"next-bundle@0.2.0 --build-command {quoted_command}"
            )
        return build_command

    def node_optimize_deps_paths(self) -> list[str]:
        if self == NodeFramework.ASTRO:
            return ["dist"]
        return []

    def start_command(self) -> Optional[str]:
        if self == NodeFramework.NEXT:
            return "node server.mjs"
        return None

    def folders_to_copy(self) -> list[str]:
        if self == NodeFramework.NEXT:
            return [".next-bundle/*"]
        return ["."]

class NodeConfig(Config):
    model_config = SettingsConfigDict(extra="ignore", env_prefix="SHIPIT_")

    use_edgejs: Optional[bool] = False
    package_manager: Optional[PackageManager] = None
    framework: Optional[NodeFramework] = None
    extra_dependencies: Set[str] = Field(default_factory=set)
    build_command: Optional[str] = None
    node_version: Optional[str] = "24"
    npm_version: Optional[str] = None
    pnpm_version: Optional[str] = None
    yarn_version: Optional[str] = None
    bun_version: Optional[str] = None
    optimize_node_dependencies: Optional[bool] = True
    # Optimize node_modules size further when targeting Edge by removing
    # executable native binaries that cannot run there anyway.
    remove_native_binaries: Optional[bool] = False
    install_requires_all_files: bool = False


class NodeProvider:
    only_build: bool = False
    FRAMEWORK_DEPENDENCIES: ClassVar[tuple[str, ...]] = (
        "next",
        "astro",
        "@react-router/dev",
        "@react-router/node",
        "@react-router/serve",
        "@remix-run/dev",
        "@remix-run/node",
        "@remix-run/react",
        "@remix-run/server-runtime",
        "@sveltejs/kit",
        "@sveltejs/adapter-node",
        "nitropack",
        "nitro",
        "@solidjs/start",
        "solid-start",
        "solid-js",
        "@tanstack/react-start",
        "@tanstack/solid-start",
        "@shopify/hydrogen",
        "@shopify/remix-oxygen",
        "@nestjs/common",
        "@nestjs/core",
        "@nestjs/platform-express",
        "@nestjs/platform-fastify",
        "hono",
        "@hono/node-server",
        "express",
        "h3",
        "koa",
        "elysia",
        "@elysia/node",
        "fastify",
        "xmcp",
        "mastra",
        "@mastra/core",
    )
    HYDROGEN_CONFIG_FILES: ClassVar[tuple[str, ...]] = (
        "hydrogen.config.js",
        "hydrogen.config.ts",
    )
    COMMON_ENTRY_FILES = (
        "server.js",
        "app.js",
        "index.js",
        "src/server.js",
        "src/index.js",
    )
    START_PROGRAMS = {
        "edge",
        "node",
        "npm",
        "npx",
        "pnpm",
        "pnpx",
        "yarn",
        "ypx",
        "bun",
        "bunx",
    }

    def __init__(
        self, path: Path, config: NodeConfig, only_build: bool = False
    ) -> None:
        self.path = path
        self.config = config
        self.only_build = only_build

    @property
    def app_subdir(self) -> Optional[str]:
        return self.config.app_subdir

    def build_workdir_steps(self, mount_name: str) -> list[str]:
        return subdir_build_context_steps(
            mount_name,
            self.app_subdir,
            extra_ignore=["node_modules"],
        )

    @classmethod
    def name(cls) -> str:
        return "node"

    @classmethod
    def load_config(
        cls,
        path: Path,
        base_config: Config,
        infer_start: bool = True,
    ) -> NodeConfig:
        config = NodeConfig(**base_config.model_dump())
        if not config.package_manager:
            config.package_manager = cls.detect_package_manager(path)

        package_json = cls.parse_package_json(path)
        install_context = discover_js_install_context(path)
        if install_context.requires_all_files:
            config.install_requires_all_files = True

        found_deps = cls._check_package_json_deps(
            package_json, *cls.FRAMEWORK_DEPENDENCIES
        )
        if not config.framework:
            config.framework = cls.detect_framework(
                package_json, found_deps, path
            )

        if not config.build_command:
            config.build_command = cls.get_build_command(
                package_json,
                config.package_manager,
                config.framework,
                explicit_build_command=config.commands.build,
            )

        if infer_start and not config.commands.start:
            if config.framework:
                config.commands.start = config.framework.start_command()
            if not config.commands.start:
                config.commands.start = cls.infer_start_command(
                    path, package_json
                )

        if config.framework and config.commands.build and config.build_command:
            config.commands.build = config.build_command

        return config

    @classmethod
    def detect(
        cls, path: Path, config: Config
    ) -> Optional[DetectResult]:
        if config.commands.start and cls._is_node_command(config.commands.start):
            return DetectResult(cls.name(), 35)

        if config.commands.install:
            install_commands = {
                "npm install",
                "npm ci",
                "npm i",
                "pnpm install",
                "pnpm ci",
                "pnpm i",
                "yarn install",
                "yarn ci",
                "yarn i",
                "bun install",
                "bun ci",
                "bun i",
            }
            if config.commands.install in install_commands:
                return DetectResult(cls.name(), 30)

        package_json = cls.parse_package_json(path)
        found_deps = cls._check_package_json_deps(
            package_json, *cls.FRAMEWORK_DEPENDENCIES
        )
        if cls.detect_framework(package_json, found_deps, path):
            return DetectResult(cls.name(), 45)

        if (path / "package.json").is_file():
            return DetectResult(cls.name(), 30)

        for entry_file in cls.COMMON_ENTRY_FILES:
            if (path / entry_file).is_file():
                return DetectResult(cls.name(), 30)

        return None

    @classmethod
    def detect_package_manager(cls, path: Path) -> PackageManager:
        package_json = cls.parse_package_json(path) or {}
        package_manager = package_json.get("packageManager")
        if isinstance(package_manager, str):
            name = package_manager.split("@", 1)[0].lower()
            for manager in PackageManager:
                if manager.value == name:
                    return manager

        if (path / "pnpm-workspace.yaml").exists():
            return PackageManager.PNPM
        if (path / "package-lock.json").exists():
            return PackageManager.NPM
        if (path / "pnpm-lock.yaml").exists():
            return PackageManager.PNPM
        if (path / "yarn.lock").exists():
            return PackageManager.YARN
        if (path / "bun.lockb").exists():
            return PackageManager.BUN
        return PackageManager.NPM

    @classmethod
    def parse_package_json(cls, path: Path) -> Optional[Dict[str, Any]]:
        package_json_path = path / "package.json"
        if not package_json_path.exists():
            return None
        try:
            package_json = json.loads(package_json_path.read_text())
            assert isinstance(package_json, dict), (
                "package.json must be a valid JSON object"
            )
            return package_json
        except Exception:
            return None

    @classmethod
    def package_scripts(
        cls, package_json: Optional[Dict[str, Any]]
    ) -> Dict[str, str]:
        if not package_json:
            return {}
        scripts = package_json.get("scripts", {})
        if not isinstance(scripts, dict):
            return {}
        return {
            name: command
            for name, command in scripts.items()
            if isinstance(name, str) and isinstance(command, str)
        }

    @classmethod
    def detect_framework(
        cls,
        package_json: Optional[Dict[str, Any]],
        found_deps: Optional[Set[str]] = None,
        path: Optional[Path] = None,
    ) -> Optional[NodeFramework]:
        found_deps = found_deps or set()

        if "next" in found_deps:
            return NodeFramework.NEXT

        elif "astro" in found_deps:
            return NodeFramework.ASTRO

        elif found_deps & {
            "@shopify/hydrogen",
            "@shopify/remix-oxygen",
        } or cls._has_hydrogen_config(path):
            return NodeFramework.HYDROGEN

        elif found_deps & {
            "@react-router/dev",
            "@react-router/node",
            "@react-router/serve",
        }:
            return NodeFramework.REACT_ROUTER

        elif found_deps & {
            "@remix-run/dev",
            "@remix-run/node",
            "@remix-run/react",
            "@remix-run/server-runtime",
        }:
            return NodeFramework.REMIX

        elif "@sveltejs/kit" in found_deps:
            return NodeFramework.SVELTEKIT

        elif found_deps & {"nitropack", "nitro"}:
            return NodeFramework.NITRO

        elif found_deps & {"@solidjs/start", "solid-start"}:
            return NodeFramework.SOLIDSTART

        elif found_deps & {"@tanstack/react-start", "@tanstack/solid-start"}:
            return NodeFramework.TANSTACK_START

        elif found_deps & {
            "@nestjs/common",
            "@nestjs/core",
            "@nestjs/platform-express",
            "@nestjs/platform-fastify",
        }:
            return NodeFramework.NESTJS

        elif found_deps & {
            "hono",
            "@hono/node-server",
        }:
            return NodeFramework.HONO

        elif "express" in found_deps:
            return NodeFramework.EXPRESS

        elif "h3" in found_deps:
            return NodeFramework.H3

        elif "koa" in found_deps:
            return NodeFramework.KOA

        elif found_deps & {"elysia", "@elysia/node"}:
            return NodeFramework.ELYSIA

        elif "fastify" in found_deps:
            return NodeFramework.FASTIFY

        elif "xmcp" in found_deps:
            return NodeFramework.XMCP

        elif found_deps & {
            "mastra",
            "@mastra/core",
        }:
            return NodeFramework.MASTRA

        for command in cls._script_commands(package_json):
            try:
                tokens = shlex.split(command)
            except ValueError:
                tokens = command.split()
            if "next" in tokens:
                return NodeFramework.NEXT

        return None

    @classmethod
    def _has_hydrogen_config(cls, path: Optional[Path]) -> bool:
        if path is None:
            return False
        return any((path / file).is_file() for file in cls.HYDROGEN_CONFIG_FILES)

    @classmethod
    def has_any_dependency(
        cls,
        path: Path,
        deps: tuple[str, ...],
    ) -> bool:
        return bool(cls.check_deps(path, *deps))

    @classmethod
    def check_deps(cls, path: Path, *deps: str) -> Set[str]:
        package_json = cls.parse_package_json(path)
        return cls._check_package_json_deps(package_json, *deps)

    @classmethod
    def _check_package_json_deps(
        cls, package_json: Optional[Dict[str, Any]], *deps: str
    ) -> Set[str]:
        if not package_json:
            return set()

        pending_deps = set(deps)
        initial_deps = set(pending_deps)
        for section in ("dependencies", "devDependencies", "peerDependencies"):
            dep_section = package_json.get(section, {})
            if not isinstance(dep_section, dict):
                continue

            found = pending_deps & dep_section.keys()
            pending_deps -= found
            if not pending_deps:
                break

        return initial_deps - pending_deps

    @classmethod
    def _script_commands(
        cls,
        package_json: Optional[Dict[str, Any]],
        preferred: tuple[str, ...] = ("build",),
    ) -> list[str]:
        scripts = cls.package_scripts(package_json)
        commands = [scripts[name] for name in preferred if name in scripts]
        if not commands:
            # Try to backfill with commands that start with the preferred commands
            commands.extend(
                command for name, command in scripts.items() if name.startswith(preferred)
            )
        return commands

    @classmethod
    def _is_package_manager_build_command(cls, command: str) -> bool:
        package_manager_prefixes = (
            "npm run ",
            "pnpm run ",
            "pnpm ",
            "yarn run ",
            "yarn ",
            "bun run ",
            "bun ",
        )
        return command.startswith(package_manager_prefixes)

    @classmethod
    def _is_node_command(cls, command: str) -> bool:
        try:
            parts = shlex.split(command)
        except ValueError:
            parts = command.split()
        return bool(parts and parts[0] in cls.START_PROGRAMS)

    @classmethod
    def has_dependency(
        cls,
        package_json: Optional[Dict[str, Any]],
        dep: str,
        version: Optional[str] = None,
    ) -> bool:
        if not package_json:
            return False
        for section in ("dependencies", "devDependencies", "peerDependencies"):
            dep_section = package_json.get(section, {})
            if dep in dep_section:
                if version:
                    try:
                        constraint = NpmSpec(dep_section[dep])
                        return Version(version) in constraint
                    except Exception:
                        pass
                else:
                    return True
        return False

    @classmethod
    def has_dependency_major(
        cls,
        package_json: Optional[Dict[str, Any]],
        dep: str,
        major: int,
    ) -> bool:
        version = cls.dependency_version(package_json, dep)
        if version is None:
            return False
        try:
            return Version(f"{major}.999.999") in NpmSpec(version)
        except Exception:
            normalized = version.lstrip("^~>=< ")
            return normalized == str(major) or normalized.startswith(
                f"{major}."
            )

    @classmethod
    def dependency_version(
        cls,
        package_json: Optional[Dict[str, Any]],
        dep: str,
    ) -> Optional[str]:
        if not package_json:
            return None
        for section in ("dependencies", "devDependencies", "peerDependencies"):
            dep_section = package_json.get(section, {})
            if not isinstance(dep_section, dict):
                continue
            version = dep_section.get(dep)
            if isinstance(version, str):
                return version
        return None

    @classmethod
    def get_build_command(
        cls,
        package_json: Optional[Dict[str, Any]],
        package_manager: PackageManager,
        framework: Optional[NodeFramework] = None,
        explicit_build_command: Optional[str] = None,
    ) -> Optional[str]:
        if explicit_build_command:
            build_command = explicit_build_command
        else:
            build_command = None

        scripts = cls.package_scripts(package_json)
        if not build_command and "build" in scripts:
            build_command = package_manager.run_command("build")
        if not build_command and framework == NodeFramework.NEXT:
            build_command = package_manager.run_execute_command("next build")

        if build_command and framework:
            return framework.bundle_build_command(package_manager, build_command)
        return build_command

    @classmethod
    def infer_start_command(
        cls, path: Path, package_json: Optional[Dict[str, Any]]
    ) -> Optional[str]:
        scripts = cls.package_scripts(package_json)
        start_script = scripts.get("start")
        if start_script:
            return start_script.strip() or None

        if package_json:
            main = package_json.get("main")
            if isinstance(main, str) and main.strip():
                return cls._node_entry_command(main.strip())

        for entry_file in cls.COMMON_ENTRY_FILES:
            if (path / entry_file).is_file():
                return cls._node_entry_command(entry_file)

        return None

    @classmethod
    def _node_entry_command(cls, entry_file: str) -> str:
        return f"node {shlex.quote(entry_file)}"

    def dependencies(self) -> list[DependencySpec]:
        node_dep = DependencySpec(
            "node",
            var_name="config.node_version",
            use_in_build=bool((self.path / "package.json").exists())
            or bool(self.config.build_command),
            use_in_serve=not self.only_build,
        )
        deps = [node_dep]

        if self.config.package_manager and (self.path / "package.json").exists():
            package_manager_dep = self.config.package_manager.as_dependency(self.path)
            package_manager_dep.use_in_build = True
            deps.append(package_manager_dep)
            if not self.only_build and self.config.remove_native_binaries:
                deps.append(DependencySpec("bash", use_in_build=True))

        for dep in sorted(self.config.extra_dependencies):
            deps.append(DependencySpec(dep, use_in_build=True))

        return deps

    def build_steps_install(self) -> list[str]:
        if not (self.path / "package.json").exists():
            return []
        package_manager = self.config.package_manager
        if package_manager is None:
            package_manager = self.detect_package_manager(self.path)
        lockfile = package_manager.lockfile()
        has_lockfile = (self.path / lockfile).exists()
        install_command = package_manager.install_command(
            has_lockfile=has_lockfile
        )
        install_context = discover_js_install_context(self.path)
        requires_all_files = (
            self.config.install_requires_all_files
            or install_context.requires_all_files
        )
        steps = []

        if self.app_subdir:
            if has_lockfile:
                steps.append(
                    f'copy("{{}}/{lockfile}".format(app_subdir), "{lockfile}")'
                )
        elif requires_all_files:
            steps.append('copy(".", ".", ignore=["node_modules", ".git"])')
        elif has_lockfile:
            steps.append(f'copy("{lockfile}")')

        if package_manager == PackageManager.PNPM:
            steps.append(
                'env('
                'pnpm_config_minimum_release_age="0", '
                'CI="true", '
                'pnpm_config_dangerously_allow_all_builds="true"'
                ')'
            )
        elif package_manager == PackageManager.NPM:
            steps.append(f'env(CI="true", NPM_CONFIG_FUND="false")')

        if self.app_subdir or requires_all_files:
            steps.append(f'run("{install_command}", group="install")')
        else:
            inputs = starlark_string_list(install_context.inputs)
            steps.append(
                f'run("{install_command}", '
                f'inputs=[{inputs}], group="install")'
            )
        return steps

    def install_uses_all_files(self) -> bool:
        install_context = discover_js_install_context(self.path)
        return (
            self.config.install_requires_all_files
            or install_context.requires_all_files
        )

    def ignored_source_files(self) -> list[str]:
        ignored_files = ["node_modules", ".git"]
        if self.config.package_manager:
            lockfile = self.config.package_manager.lockfile()
            if (self.path / lockfile).exists():
                ignored_files.append(lockfile)
        return ignored_files

    def build_steps_copy(self) -> Optional[str]:
        if self.app_subdir:
            return None
        if self.install_uses_all_files():
            return None
        ignored = ", ".join(
            json.dumps(file) for file in self.ignored_source_files()
        )
        return f'copy(".", ignore=[{ignored}])'

    def build_steps_build(self, output: Optional[str] = "\".\"") -> list[str]:
        if not self.config.build_command:
            return []
        command = json.dumps(self.config.build_command)
        if not self.only_build:
            return [
                f"run({command}, "
                f"outputs=[{output}], group=\"build\")"
            ]
        return [f"run({command}, group=\"build\")"]

    def build_steps_optimize_deps(self) -> list[str]:
        if not (self.path / "package.json").exists():
            return []
        package_manager = self.config.package_manager
        if package_manager is None:
            return []
        steps = [
            f"run(\"{package_manager.prune_command()}\", group=\"prune\")"
        ]
        if self.config.framework and self.config.optimize_node_dependencies:
            node_optimize_deps_paths = self.config.framework.node_optimize_deps_paths()
            if node_optimize_deps_paths:
                optimize_deps_command = package_manager.dlx_command(
                    f"optimize-deps@{OPTIMIZE_DEPS_VERSION} {', '.join(node_optimize_deps_paths)} --replace"
                )
                steps.append(f"run(\"{optimize_deps_command}\")")
        if not self.only_build and self.config.remove_native_binaries:
            steps.extend(
                [
                    'run("mkdir -p {}".format(assets.path), group="optimize")',
                    (
                        f'copy("{NODE_MODULES_OPTIMIZER_ASSET}", '
                        '"{}/optimize-node-modules.sh".format(assets.path), '
                        'base="assets")'
                    ),
                    (
                        'run("bash {}/optimize-node-modules.sh '
                        'node_modules".format(assets.path), group="optimize")'
                    ),
                ]
            )
        return steps

    def build_steps(self) -> list[str]:
        folders_to_copy = "."
        if self.config.framework:
            folders_to_copy = ", ".join(self.config.framework.folders_to_copy())
        copy_source = folders_to_copy or "."
        return list(filter(None, [
            *self.build_workdir_steps("build"),
            *self.build_steps_install(),
            self.build_steps_copy(),
            *self.build_steps_build(),
            *self.build_steps_optimize_deps(),
            f'run("cp -R {copy_source} {{}}".format(app.path))',
        ]))

    def declarations(self) -> Optional[str]:
        return None

    def prepare_steps(self) -> Optional[list[str]]:
        return None

    def commands(self) -> Dict[str, str]:
        if not self.config.commands.start:
            return {}
        return {"start": json.dumps(self.config.commands.start)}

    def mounts(self) -> list[MountSpec]:
        if self.only_build:
            return []
        mounts = [MountSpec("build", attach_to_serve=False), MountSpec("app")]
        if self.config.remove_native_binaries:
            mounts.append(MountSpec("assets", attach_to_serve=False))
        return mounts

    def volumes(self) -> list[VolumeSpec]:
        return []

    def env(self) -> Optional[Dict[str, str]]:
        if self.only_build:
            return None
        return {}

    def services(self) -> list[ServiceSpec]:
        return []
