import json
import shlex
from enum import Enum
from pathlib import Path
from typing import Any, Dict, Optional, Set

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
)


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
    NEXT = "next"

    def bundle_build_command(
        self, package_manager: PackageManager, build_command: str
    ) -> str:
        if self == NodeFramework.NEXT:
            quoted_command = shlex.quote(build_command)
            return package_manager.dlx_command(
                f"next-bundle --build-command {quoted_command}"
            )
        raise NotImplementedError(f"Unsupported Node framework: {self.value}")

    def start_command(self) -> str:
        if self == NodeFramework.NEXT:
            return "node .next-bundle/server.mjs"
        raise NotImplementedError(f"Unsupported Node framework: {self.value}")


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


class NodeProvider:
    only_build: bool = False
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
        if not config.framework:
            config.framework = cls.detect_framework(package_json)

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
            else:
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
        if cls.detect_framework(package_json):
            return DetectResult(cls.name(), 45)

        if (path / "package.json").is_file():
            return DetectResult(cls.name(), 30)

        for entry_file in cls.COMMON_ENTRY_FILES:
            if (path / entry_file).is_file():
                return DetectResult(cls.name(), 30)

        return None

    @classmethod
    def detect_package_manager(cls, path: Path) -> PackageManager:
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
        cls, package_json: Optional[Dict[str, Any]]
    ) -> Optional[NodeFramework]:
        if cls.has_dependency(package_json, "next"):
            return NodeFramework.NEXT

        for command in cls._script_commands(package_json):
            try:
                tokens = shlex.split(command)
            except ValueError:
                tokens = command.split()
            if "next" in tokens:
                return NodeFramework.NEXT

        return None

    @classmethod
    def _script_commands(
        cls,
        package_json: Optional[Dict[str, Any]],
        preferred: tuple[str, ...] = ("build",),
    ) -> list[str]:
        scripts = cls.package_scripts(package_json)
        commands = [scripts[name] for name in preferred if name in scripts]
        commands.extend(
            command for name, command in scripts.items() if name not in preferred
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
        return list(
            filter(
                None,
                [
                    f'copy("{lockfile}")' if has_lockfile else None,
                    (
                        'env(CI="true", NPM_CONFIG_FUND="false")'
                    )
                    if package_manager == PackageManager.NPM
                    else None,
                    (
                        f'run("{install_command}", inputs=["package.json"], '
                        'group="install")'
                    ),
                ],
            )
        )

    def ignored_source_files(self) -> list[str]:
        ignored_files = ["node_modules", ".git"]
        if self.config.package_manager:
            lockfile = self.config.package_manager.lockfile()
            if (self.path / lockfile).exists():
                ignored_files.append(lockfile)
        return ignored_files

    def build_steps_copy(self) -> str:
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

    def build_steps_prune(self) -> list[str]:
        if not (self.path / "package.json").exists():
            return []
        package_manager = self.config.package_manager
        if package_manager is None:
            return []
        return [f"run(\"{package_manager.prune_command()}\", group=\"prune\")"]

    def build_steps(self) -> list[str]:
        return [
            "workdir(app.path)",
            *self.build_steps_install(),
            self.build_steps_copy(),
            *self.build_steps_build(),
            *self.build_steps_prune(),
        ]

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
        return [MountSpec("app")]

    def volumes(self) -> list[VolumeSpec]:
        return []

    def env(self) -> Optional[Dict[str, str]]:
        if self.only_build:
            return None
        return {"PORT": "PORT"}

    def services(self) -> list[ServiceSpec]:
        return []
