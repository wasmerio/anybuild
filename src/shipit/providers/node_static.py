import json
import re
import shlex
from enum import Enum
from pathlib import Path
from typing import Any, Dict, List, Optional, Set

import yaml
from pydantic import Field
from pydantic_settings import SettingsConfigDict
from semantic_version import NpmSpec, Version

from .base import (
    Config,
    DetectResult,
    DependencySpec,
    MountSpec,
    Provider,
    ServiceSpec,
    VolumeSpec,
    _exists,
)
from .staticfile import StaticFileProvider, StaticFileConfig


class PackageManager(Enum):
    NPM = "npm"
    PNPM = "pnpm"
    YARN = "yarn"
    BUN = "bun"

    def as_dependency(self, path) -> DependencySpec:
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
        # Read line by line and return the lockfileVersion
        with open(lockfile, "r") as f:
            for line in f:
                if "lockfileVersion" in line:
                    try:
                        config = yaml.safe_load(line)
                        version = config.get("lockfileVersion")
                        assert isinstance(version, (str, bytes))
                        return version
                    except:
                        pass
        return None

    def install_command(self, has_lockfile: bool = False) -> str:
        return {
            PackageManager.NPM: f"npm {'ci' if has_lockfile else 'install'}",
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


class StaticGenerator(Enum):
    ASTRO = "astro"
    VITE = "vite"
    NEXT = "next"
    GATSBY = "gatsby"
    ELEVENTY = "eleventy"
    VITEPRESS = "vitepress"
    VUEPRESS = "vuepress"
    HEXO = "hexo"
    METALSMITH = "metalsmith"
    ASSEMBLE = "assemble"
    HARP = "harp"
    DOCUSAURUS_OLD = "docusaurus-old"
    DOCUSAURUS = "docusaurus"
    SVELTE = "svelte"
    REMIX = "remix"
    NUXT_OLD = "nuxt"
    NUXT_V3 = "nuxt3"
    REMIX_OLD = "remix-old"
    REMIX_V2 = "remix-v2"
    REMIX_V2_CLASSIC = "remix-v2-classic"

    def get_output_dir(self) -> str:
        if self == StaticGenerator.NEXT:
            return "out"
        elif self == StaticGenerator.ELEVENTY:
            return "_site"
        elif self == StaticGenerator.NUXT_V3:
            return ".output/public"
        elif self in [
            StaticGenerator.ASTRO,
            StaticGenerator.VITE,
            StaticGenerator.NUXT_OLD,
        ]:
            return "dist"
        elif self == StaticGenerator.GATSBY:
            return "public"
        elif self == StaticGenerator.HEXO:
            return "public"
        elif self == StaticGenerator.VITEPRESS:
            return "docs/.vitepress/dist"
        elif self == StaticGenerator.VUEPRESS:
            return "docs/.vuepress/dist"
        elif self in [
            StaticGenerator.REMIX_OLD,
            StaticGenerator.REMIX,
            StaticGenerator.REMIX_V2,
        ]:
            return "build/client"
        elif self == StaticGenerator.REMIX_V2_CLASSIC:
            return "public"
        elif self == StaticGenerator.ASSEMBLE:
            return "dist"
        elif self == StaticGenerator.HARP:
            return "www"
        elif self in [
            StaticGenerator.DOCUSAURUS,
            StaticGenerator.DOCUSAURUS_OLD,
            StaticGenerator.SVELTE,
            StaticGenerator.METALSMITH,
        ]:
            return "build"
        else:
            return "dist"

    @classmethod
    def detect_generators_from_command(
        cls, build_command
    ) -> List["StaticGenerator"]:
        commands = {
            "gatsby": [StaticGenerator.GATSBY],
            "astro": [StaticGenerator.ASTRO],
            "@11ty/eleventy": [StaticGenerator.ELEVENTY],
            "eleventy": [StaticGenerator.ELEVENTY],
            "remix-ssg": [StaticGenerator.REMIX_OLD],
            "remix": [StaticGenerator.REMIX_V2_CLASSIC, StaticGenerator.REMIX_V2],
            "vite": [StaticGenerator.VITE],
            "vitepress": [StaticGenerator.VITEPRESS],
            "vuepress": [StaticGenerator.VUEPRESS],
            "hexo": [StaticGenerator.HEXO],
            "metalsmith": [StaticGenerator.METALSMITH],
            "harp": [StaticGenerator.HARP],
            "docusaurus": [
                StaticGenerator.DOCUSAURUS,
                StaticGenerator.DOCUSAURUS_OLD,
            ],
            "next": [StaticGenerator.NEXT],
            "nuxi": [StaticGenerator.NUXT_V3],
            "nuxt": [StaticGenerator.NUXT_OLD],
            "svelte-kit": [StaticGenerator.SVELTE],
        }
        try:
            tokens = shlex.split(build_command)
        except ValueError:
            tokens = build_command.split()

        for index, token in enumerate(tokens):
            if token == "grunt" and "assemble" in tokens[index + 1 :]:
                return [StaticGenerator.ASSEMBLE]
            if token in commands:
                return commands[token]
        return []

    def build_command(self) -> str:
        return {
            StaticGenerator.GATSBY: "gatsby build",
            StaticGenerator.ELEVENTY: "@11ty/eleventy",
            StaticGenerator.VITEPRESS: "vitepress build docs",
            StaticGenerator.VUEPRESS: "vuepress build docs",
            StaticGenerator.HEXO: "hexo generate",
            StaticGenerator.METALSMITH: "metalsmith build",
            StaticGenerator.ASSEMBLE: "grunt assemble",
            StaticGenerator.HARP: "harp compile . www",
            StaticGenerator.ASTRO: "astro build",
            StaticGenerator.REMIX_OLD: "remix-ssg build",
            StaticGenerator.REMIX_V2: "vite build",
            StaticGenerator.REMIX_V2_CLASSIC: "remix build",
            StaticGenerator.DOCUSAURUS: "docusaurus build",
            StaticGenerator.DOCUSAURUS_OLD: "docusaurus build",
            StaticGenerator.SVELTE: "svelte-kit build",
            StaticGenerator.VITE: "vite build",
            StaticGenerator.NEXT: "next export",
            StaticGenerator.NUXT_V3: "nuxi generate",
            StaticGenerator.NUXT_OLD: "nuxt generate",
            StaticGenerator.REMIX: "remix build",
        }[self]


class NodeStaticConfig(StaticFileConfig):
    model_config = SettingsConfigDict(extra="ignore", env_prefix="SHIPIT_")

    package_manager: Optional[PackageManager] = None
    extra_dependencies: Set[str] = Field(default_factory=set)
    static_generator: Optional[StaticGenerator] = None
    build_command: Optional[str] = None
    node_version: Optional[str] = "22"
    npm_version: Optional[str] = None
    pnpm_version: Optional[str] = None
    yarn_version: Optional[str] = None
    bun_version: Optional[str] = None


class NodeStaticProvider(StaticFileProvider):
    only_build: bool = False
    _ASSEMBLE_DEST_PATTERN = re.compile(r"\bdest\s*:\s*['\"]([^'\"]+)['\"]")

    def __init__(
        self, path: Path, config: NodeStaticConfig, only_build: bool = False
    ):
        super().__init__(path, config)
        self.only_build = only_build

    @classmethod
    def load_config(
        cls, path: Path, base_config: Config
    ) -> NodeStaticConfig:
        config = NodeStaticConfig(**base_config.model_dump())
        if not config.package_manager:
            if (path / "package-lock.json").exists():
                config.package_manager = PackageManager.NPM
            elif (path / "pnpm-lock.yaml").exists():
                config.package_manager = PackageManager.PNPM
            elif (path / "yarn.lock").exists():
                config.package_manager = PackageManager.YARN
            elif (path / "bun.lockb").exists():
                config.package_manager = PackageManager.BUN
            else:
                config.package_manager = PackageManager.PNPM

        package_json = cls.parse_package_json(path)

        if not config.static_generator:
            if cls.has_dependency(package_json, "@11ty/eleventy"):
                config.static_generator = StaticGenerator.ELEVENTY
            elif cls.has_dependency(package_json, "vitepress"):
                config.static_generator = StaticGenerator.VITEPRESS
            elif cls.has_dependency(package_json, "vuepress"):
                config.static_generator = StaticGenerator.VUEPRESS
            elif cls.has_dependency(package_json, "hexo") or cls.has_dependency(
                package_json, "hexo-cli"
            ):
                config.static_generator = StaticGenerator.HEXO
            elif cls.has_dependency(package_json, "metalsmith"):
                config.static_generator = StaticGenerator.METALSMITH
            elif cls.has_dependency(package_json, "assemble") or cls.has_dependency(
                package_json, "grunt-assemble"
            ):
                config.static_generator = StaticGenerator.ASSEMBLE
            elif cls.has_dependency(package_json, "harp"):
                config.static_generator = StaticGenerator.HARP
            elif cls.has_dependency(package_json, "gatsby"):
                config.static_generator = StaticGenerator.GATSBY
            elif cls.has_dependency(package_json, "astro"):
                config.static_generator = StaticGenerator.ASTRO
            elif cls.has_dependency(package_json, "docusaurus"):
                config.static_generator = StaticGenerator.DOCUSAURUS_OLD
            elif cls.has_dependency(package_json, "@docusaurus/core"):
                config.static_generator = StaticGenerator.DOCUSAURUS
            elif cls.has_dependency(package_json, "svelte"):
                config.static_generator = StaticGenerator.SVELTE
            elif cls.has_dependency(
                package_json, "@remix-run/dev", "1"
            ) or cls.has_dependency(package_json, "@remix-run/dev", "0"):
                config.static_generator = StaticGenerator.REMIX_OLD
            elif cls.has_dependency(package_json, "@remix-run/dev"):
                has_vite = (
                    cls.has_dependency(package_json, "@remix-run/vite")
                    or cls.has_dependency(package_json, "vite")
                    or _exists(
                        path,
                        "vite.config.js",
                        "vite.config.ts",
                        "vite.config.mjs",
                        "vite.config.cjs",
                    )
                )
                if has_vite:
                    config.static_generator = StaticGenerator.REMIX_V2
                else:
                    config.static_generator = StaticGenerator.REMIX_V2_CLASSIC
            elif cls.has_dependency(package_json, "vite"):
                config.static_generator = StaticGenerator.VITE
            elif cls.has_dependency(package_json, "next"):
                config.static_generator = StaticGenerator.NEXT
            elif cls.has_dependency(package_json, "nuxt", "2") or cls.has_dependency(
                package_json, "nuxt", "1"
            ):
                config.static_generator = StaticGenerator.NUXT_OLD
            elif cls.has_dependency(package_json, "nuxt"):
                config.static_generator = StaticGenerator.NUXT_V3

        if not config.build_command:
            config.build_command = cls.get_build_command(
                package_json, config.package_manager, config.static_generator
            )

        if not config.static_dir:
            if config.static_generator:
                config.static_dir = cls.get_static_dir(
                    path, package_json, config.static_generator
                )
            else:
                config.static_dir = "dist"

        return config

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
    def get_static_dir(
        cls,
        path: Path,
        package_json: Optional[Dict[str, Any]],
        static_generator: StaticGenerator,
    ) -> str:
        if static_generator == StaticGenerator.VITEPRESS:
            root = cls._script_build_root(package_json, "vitepress")
            root = root or cls._default_docs_root(path, ".vitepress")
            return cls._rooted_output_dir(root, ".vitepress/dist")

        if static_generator == StaticGenerator.VUEPRESS:
            root = cls._script_build_root(package_json, "vuepress")
            root = root or cls._default_docs_root(path, ".vuepress")
            return cls._rooted_output_dir(root, ".vuepress/dist")

        if static_generator == StaticGenerator.METALSMITH:
            return cls._metalsmith_output_dir(path) or static_generator.get_output_dir()

        if static_generator == StaticGenerator.ASSEMBLE:
            return cls._assemble_output_dir(path) or static_generator.get_output_dir()

        if static_generator == StaticGenerator.HARP:
            return (
                cls._harp_output_dir(package_json)
                or static_generator.get_output_dir()
            )

        return static_generator.get_output_dir()

    @classmethod
    def _script_commands(
        cls, package_json: Optional[Dict[str, Any]]
    ) -> list[str]:
        if not package_json:
            return []
        scripts = package_json.get("scripts", {})
        if not isinstance(scripts, dict):
            return []

        preferred = ("generate", "build", "docs:build")
        commands = [
            scripts[name]
            for name in preferred
            if isinstance(scripts.get(name), str)
        ]
        commands.extend(
            command
            for name, command in scripts.items()
            if name not in preferred and isinstance(command, str)
        )
        return commands

    @classmethod
    def _args_after_command(cls, command: str, executable: str) -> list[str]:
        try:
            tokens = shlex.split(command)
        except ValueError:
            tokens = command.split()

        for index, token in enumerate(tokens):
            if token == executable:
                return tokens[index + 1 :]
        return []

    @classmethod
    def _script_build_root(
        cls, package_json: Optional[Dict[str, Any]], executable: str
    ) -> Optional[str]:
        for command in cls._script_commands(package_json):
            args = cls._args_after_command(command, executable)
            if not args or args[0] != "build":
                continue
            for arg in args[1:]:
                if not arg.startswith("-"):
                    return cls._clean_output_dir(arg)
        return None

    @classmethod
    def _default_docs_root(cls, path: Path, config_dir: str) -> str:
        docs_path = path / "docs"
        if (docs_path / config_dir).exists() or docs_path.exists():
            return "docs"
        return "."

    @classmethod
    def _rooted_output_dir(cls, root: str, output_dir: str) -> str:
        root = cls._clean_output_dir(root)
        if root == ".":
            return output_dir
        return f"{root}/{output_dir}"

    @classmethod
    def _clean_output_dir(cls, output_dir: str) -> str:
        output_dir = output_dir.strip().rstrip("/")
        if output_dir.startswith("./"):
            output_dir = output_dir[2:]
        return output_dir or "."

    @classmethod
    def _metalsmith_output_dir(cls, path: Path) -> Optional[str]:
        for config_name in ("metalsmith.json", ".metalsmith.json"):
            config_path = path / config_name
            if not config_path.is_file():
                continue
            try:
                config = json.loads(config_path.read_text())
            except Exception:
                continue
            if not isinstance(config, dict):
                continue
            for key in ("destination", "dest"):
                output_dir = config.get(key)
                if isinstance(output_dir, str) and output_dir:
                    return cls._clean_output_dir(output_dir)
        return None

    @classmethod
    def _assemble_output_dir(cls, path: Path) -> Optional[str]:
        for config_name in ("Gruntfile.js", "Gruntfile.cjs"):
            config_path = path / config_name
            if not config_path.is_file():
                continue
            match = cls._ASSEMBLE_DEST_PATTERN.search(config_path.read_text())
            if match:
                return cls._clean_output_dir(match.group(1))
        return None

    @classmethod
    def _harp_output_dir(
        cls, package_json: Optional[Dict[str, Any]]
    ) -> Optional[str]:
        for command in cls._script_commands(package_json):
            args = cls._args_after_command(command, "harp")
            if not args:
                continue
            if args[0] == "compile":
                args = args[1:]
            positional_args = []
            for index, arg in enumerate(args):
                if arg in ("--output", "-o") and index + 1 < len(args):
                    return cls._clean_output_dir(args[index + 1])
                if not arg.startswith("-"):
                    positional_args.append(arg)
            if len(positional_args) >= 2:
                return cls._clean_output_dir(positional_args[1])
        return None

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
    def name(cls) -> str:
        return "node-static"

    @classmethod
    def detect(
        cls, path: Path, config: Config
    ) -> Optional[DetectResult]:
        if config.commands.install:
            # Detect this provider from the install command
            install_commands = [
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
            ]
            if config.commands.install in install_commands:
                return DetectResult(cls.name(), 40)

        has_package_manager_build_command = False
        if config.commands.build:
            # Iterate over all generators and check if the build command matches
            for static_generator in StaticGenerator:
                if static_generator.build_command() in config.commands.build:
                    return DetectResult(cls.name(), 60)

            static_generators = StaticGenerator.detect_generators_from_command(
                config.commands.build
            )
            if static_generators:
                return DetectResult(cls.name(), 60)

            has_package_manager_build_command = (
                cls._is_package_manager_build_command(config.commands.build)
            )

        package_json = cls.parse_package_json(path)
        if not package_json:
            if has_package_manager_build_command:
                return DetectResult(cls.name(), 40)
            return None
        for build_command in cls._script_commands(package_json):
            for static_generator in StaticGenerator:
                if static_generator.build_command() in build_command:
                    return DetectResult(cls.name(), 60)
            if StaticGenerator.detect_generators_from_command(build_command):
                return DetectResult(cls.name(), 60)

        pure_static_generators = [
            "@11ty/eleventy",
            "vitepress",
            "vuepress",
            "hexo",
            "hexo-cli",
            "metalsmith",
            "assemble",
            "grunt-assemble",
            "harp",
            "docusaurus",
            "@docusaurus/core",
        ]
        static_generators = [
            "astro",
            "vite",
            "next",
            "nuxt",
            "gatsby",
            "svelte",
            "@remix-run/dev",
        ]
        if any(cls.has_dependency(package_json, dep) for dep in pure_static_generators):
            return DetectResult(cls.name(), 60)
        if any(cls.has_dependency(package_json, dep) for dep in static_generators):
            return DetectResult(cls.name(), 40)
        if has_package_manager_build_command:
            return DetectResult(cls.name(), 40)
        return None

    def dependencies(self) -> list[DependencySpec]:
        package_manager_dep = self.config.package_manager.as_dependency(self.path)
        package_manager_dep.use_in_build = True
        return [
            DependencySpec(
                "node",
                var_name="config.node_version",
                use_in_build=True,
            ),
            package_manager_dep,
            *(super().dependencies() if not self.only_build else []),
        ]

    @classmethod
    def get_build_command(
        cls,
        package_json: Optional[Dict[str, Any]],
        package_manager: PackageManager,
        static_generator: Optional[StaticGenerator],
    ) -> Optional[str]:
        if package_json:
            scripts = package_json.get("scripts", {})
            if not isinstance(scripts, dict):
                scripts = {}
            docs_build_command = scripts.get("docs:build")
            if (
                static_generator
                in [StaticGenerator.VITEPRESS, StaticGenerator.VUEPRESS]
                and docs_build_command
            ):
                return package_manager.run_command("docs:build")
            generate_command = scripts.get("generate")
            if generate_command:
                return package_manager.run_command("generate")
            build_command = scripts.get("build")
            if build_command:
                return package_manager.run_command("build")
            if docs_build_command:
                return package_manager.run_command("docs:build")
        if not static_generator:
            return None
        command = static_generator.build_command()
        return package_manager.run_execute_command(command)

    def build_steps_install(self) -> list[str]:
        lockfile = self.config.package_manager.lockfile()
        has_lockfile = (self.path / lockfile).exists()
        install_command = self.config.package_manager.install_command(
            has_lockfile=has_lockfile
        )
        return filter(
            None,
            [
                f'copy("{lockfile}")' if has_lockfile else None,
                'env(CI="true", NODE_ENV="production", NPM_CONFIG_FUND="false")'
                if self.config.package_manager == PackageManager.NPM
                else None,
                f'run("{install_command}", inputs=["package.json"], group="install")',
            ],
        )

    def build_steps_build(self) -> list[str]:
        return filter(
            None,
            [
                (
                    f'run("{self.config.build_command}", '
                    'outputs=[config.static_dir], group="build")'
                )
                if self.config.build_command and not self.only_build
                else None,
                f'run("{self.config.build_command}", group="build")'
                if self.config.build_command and self.only_build
                else None,
            ]
        )

    def build_steps(self) -> list[str]:
        lockfile = self.config.package_manager.lockfile()
        has_lockfile = (self.path / lockfile).exists()
        install_command = self.config.package_manager.install_command(
            has_lockfile=has_lockfile
        )
        ignored_files = ["node_modules", ".git"]
        if has_lockfile:
            ignored_files.append(lockfile)
        all_ignored_files = ", ".join([f'"{file}"' for file in ignored_files])

        return filter(
            None,
            [
                'workdir(temp.path)' if not self.only_build else None,
                *self.build_steps_install(),
                f'copy(".", ignore=[{all_ignored_files}])',
                *self.build_steps_build(),
                'run("cp -R {}/* {}/".format(config.static_dir, static_app.path))'
                if not self.only_build
                else None,
            ] + self.build_steps_redirects(),
        )

    def prepare_steps(self) -> Optional[list[str]]:
        return None

    def mounts(self) -> list[MountSpec]:
        if self.only_build:
            return []
        return [MountSpec("temp", attach_to_serve=False), *super().mounts()]

    def volumes(self) -> list[VolumeSpec]:
        return []

    def env(self) -> Optional[Dict[str, str]]:
        return None

    def services(self) -> list[ServiceSpec]:
        return []
