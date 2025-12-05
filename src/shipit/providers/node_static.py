import json
import yaml
from pathlib import Path
from typing import Dict, Optional, Any, Set, List
from enum import Enum
from semantic_version import Version, NpmSpec
from pydantic import Field


from .base import (
    DetectResult,
    DependencySpec,
    Provider,
    MountSpec,
    ServiceSpec,
    VolumeSpec,
    Config,
)
from .staticfile import StaticFileProvider, StaticFileConfig
from pydantic_settings import SettingsConfigDict


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
    DOCUSAURUS_OLD = "docusaurus-old"
    DOCUSAURUS = "docusaurus"
    SVELTE = "svelte"
    REMIX = "remix"
    NUXT_OLD = "nuxt"
    NUXT_V3 = "nuxt3"
    REMIX_OLD = "remix-old"
    REMIX_V2 = "remix-v2"

    def get_output_dir(self) -> str:
        if self == StaticGenerator.NEXT:
            return "out"
        elif self == StaticGenerator.NUXT_V3:
            return ".output/public"
        elif self in [
            StaticGenerator.ASTRO,
            StaticGenerator.VITE,
            StaticGenerator.NUXT_OLD,
            StaticGenerator.REMIX_V2,
        ]:
            return "dist"
        elif self == StaticGenerator.GATSBY:
            return "public"
        elif self == StaticGenerator.REMIX_OLD:
            return "build/client"
        elif self in [
            StaticGenerator.DOCUSAURUS,
            StaticGenerator.DOCUSAURUS_OLD,
            StaticGenerator.SVELTE,
        ]:
            return "build"
        else:
            return "dist"

    @classmethod
    def detect_generators_from_command(cls, build_command) -> List["StaticGenerator"]:
        commands = {
            "gatsby": [StaticGenerator.GATSBY],
            "astro": [StaticGenerator.ASTRO],
            "remix-ssg": [StaticGenerator.REMIX_OLD],
            "vite": [StaticGenerator.VITE, StaticGenerator.REMIX_V2],
            "docusaurus": [StaticGenerator.DOCUSAURUS, StaticGenerator.DOCUSAURUS_OLD],
            "next": [StaticGenerator.NEXT],
            "nuxi": [StaticGenerator.NUXT_V3],
            "nuxt": [StaticGenerator.NUXT_OLD],
            "svelte-kit": [StaticGenerator.SVELTE],
        }
        build_command = build_command.split(" ")[0]
        if build_command in commands:
            return commands[build_command]
        else:
            return []
    
    def build_command(self) -> str:
        return {
            StaticGenerator.GATSBY: "gatsby build",
            StaticGenerator.ASTRO: "astro build",
            StaticGenerator.REMIX_OLD: "remix-ssg build",
            StaticGenerator.REMIX_V2: "vite build",
            StaticGenerator.DOCUSAURUS: "docusaurus build",
            StaticGenerator.DOCUSAURUS_OLD: "docusaurus build",
            StaticGenerator.SVELTE: "svelte-kit build",
            StaticGenerator.VITE: "vite build",
            StaticGenerator.NEXT: "next export",
            StaticGenerator.NUXT_V3: "nuxi generate",
            StaticGenerator.NUXT_OLD: "nuxt generate",
            StaticGenerator.REMIX: "remix-ssg build",
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
            if cls.has_dependency(package_json, "gatsby"):
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
                config.static_generator = StaticGenerator.REMIX_V2
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
            config.static_dir = config.static_generator.get_output_dir()

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
            if config.commands.install in ["npm install", "npm ci", "npm i", "pnpm install", "pnpm ci", "pnpm i", "yarn install", "yarn ci", "yarn i", "bun install", "bun ci", "bun i"]:
                return DetectResult(cls.name(), 40)

        if config.commands.build:
            if config.commands.build.startswith("npm run "):
                return DetectResult(cls.name(), 40)
            elif config.commands.build.startswith("pnpm run "):
                return DetectResult(cls.name(), 40)
            elif config.commands.build.startswith("yarn run "):
                return DetectResult(cls.name(), 40)
            elif config.commands.build.startswith("bun run "):
                return DetectResult(cls.name(), 40)

            static_generators = StaticGenerator.detect_generators_from_command(config.commands.build)
            if static_generators:
                return DetectResult(cls.name(), 40)

        package_json = cls.parse_package_json(path)
        if not package_json:
            return None
        static_generators = [
            "astro",
            "vite",
            "next",
            "nuxt",
            "gatsby",
            "svelte",
            "docusaurus",
            "@docusaurus/core",
            "@remix-run/dev",
        ]
        if any(cls.has_dependency(package_json, dep) for dep in static_generators):
            return DetectResult(cls.name(), 40)
        return None

    def serve_name(self) -> Optional[str]:
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
            *super().dependencies(),
        ]

    @classmethod
    def get_build_command(
        cls,
        package_json: Optional[Dict[str, Any]],
        package_manager: PackageManager,
        static_generator: StaticGenerator,
    ) -> Optional[str]:
        if package_json:
            scripts = package_json.get("scripts", {})
            generate_command = scripts.get("generate")
            if generate_command:
                return package_manager.run_command("generate")
            build_command = scripts.get("build")
            if build_command:
                return package_manager.run_command("build")
        command = static_generator.build_command()
        return package_manager.run_execute_command(command)

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
                f'copy("{lockfile}")' if has_lockfile else None,
                'env(CI="true", NODE_ENV="production", NPM_CONFIG_FUND="false")'
                if self.config.package_manager == PackageManager.NPM
                else None,
                # 'run("npx corepack enable", inputs=["package.json"], group="install")',
                f'run("{install_command}", inputs=["package.json"], group="install")',
                f'copy(".", ignore=[{all_ignored_files}])',
                f'run("{self.config.build_command}", outputs=[config.static_dir], group="build")'
                if not self.only_build
                else None,
                f'run("{self.config.build_command}", group="build")'
                if self.only_build
                else None,
                'run("cp -R {}/* {}/".format(config.static_dir, static_app.path))'
                if not self.only_build
                else None,
            ],
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
