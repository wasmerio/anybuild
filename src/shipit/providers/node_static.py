from __future__ import annotations

import json
from pathlib import Path
from typing import Dict, Optional, Any
from enum import Enum
from semantic_version import Version, NpmSpec


from .base import (
    DetectResult,
    DependencySpec,
    Provider,
    _exists,
    MountSpec,
    ServiceSpec,
    VolumeSpec,
    CustomCommands,
)
from .staticfile import StaticFileProvider


class PackageManager(Enum):
    NPM = "npm"
    PNPM = "pnpm"
    YARN = "yarn"
    BUN = "bun"

    def mise_dependency(self) -> list[str]:
        return {
            PackageManager.NPM: "npm",
            PackageManager.PNPM: "pnpm",
            PackageManager.YARN:  "yarn",
            PackageManager.BUN: "bun",
        }[self]

    def install_command(self) -> (str, list[str]):
        return {
            PackageManager.NPM: ("npm ci", ["package.json", "package-lock.json"]),
            PackageManager.PNPM: ("pnpm install --frozen-lockfile", ["package.json", "pnpm-lock.yaml"]),
            PackageManager.YARN: ("yarn install --frozen-lockfile", ["package.json", "yarn.lock"]),
            PackageManager.BUN: ("bun install --no-save", ["package.json", "bun.lockb"]),
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
    NUXT_OLD = "nuxt"
    NUXT_V3 = "nuxt3"


class NodeStaticProvider(StaticFileProvider):
    package_manager: PackageManager
    package_json: Optional[Dict[str, Any]]

    def __init__(self, path: Path, custom_commands: CustomCommands):
        super().__init__(path, custom_commands)
        if (path / "package-lock.json").exists():
            self.package_manager = PackageManager.NPM
        elif (path / "pnpm-lock.yaml").exists():
            self.package_manager = PackageManager.PNPM
        elif (path / "yarn.lock").exists():
            self.package_manager = PackageManager.YARN
        elif (path / "bun.lockb").exists():
            self.package_manager = PackageManager.BUN
        else:
            self.package_manager = PackageManager.PNPM

        self.package_json = self.parse_package_json(path)

        if self.has_dependency(self.package_json, "astro"):
            self.static_generator = StaticGenerator.ASTRO
        elif self.has_dependency(self.package_json, "vite"):
            self.static_generator = StaticGenerator.VITE
        elif self.has_dependency(self.package_json, "next"):
            self.static_generator = StaticGenerator.NEXT
        elif self.has_dependency(self.package_json, "nuxt", "2") or self.has_dependency(
            self.package_json, "nuxt", "1"
        ):
            self.static_generator = StaticGenerator.NUXT_OLD
        elif self.has_dependency(self.package_json, "nuxt"):
            self.static_generator = StaticGenerator.NUXT_V3

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
        cls, package_json: Optional[Dict[str, Any]], dep: str, version: Optional[str] = None
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
        cls, path: Path, custom_commands: CustomCommands
    ) -> Optional[DetectResult]:
        package_json = cls.parse_package_json(path)
        if not package_json:
            return None
        static_generators = ["astro", "vite", "next", "nuxt"]
        if any(cls.has_dependency(package_json, dep) for dep in static_generators):
            return DetectResult(cls.name(), 40)
        return None

    def initialize(self) -> None:
        pass

    def serve_name(self) -> str:
        return self.path.name

    def provider_kind(self) -> str:
        return "staticsite"

    def dependencies(self) -> list[DependencySpec]:
        return [
            DependencySpec(
                "node",
                env_var="SHIPIT_NODE_VERSION",
                default_version="22",
                use_in_build=True,
            ),
            DependencySpec(self.package_manager.mise_dependency(), use_in_build=True),
            DependencySpec("static-web-server", use_in_serve=True),
        ]

    def declarations(self) -> Optional[str]:
        return None

    def get_output_dir(self) -> str:
        if self.static_generator == StaticGenerator.ASTRO:
            return "dist"
        elif self.static_generator == StaticGenerator.VITE:
            return "dist"
        elif self.static_generator == StaticGenerator.NEXT:
            return "out"
        elif self.static_generator == StaticGenerator.NUXT:
            return "dist"
        else:
            return "dist"

    def get_build_command(self) -> bool:
        if not self.package_json:
            return False
        build_command = self.package_json.get("scripts", {}).get("build")
        if build_command:
            return self.package_manager.run_command("build")
        if self.static_generator == StaticGenerator.ASTRO:
            return self.package_manager.run_execute_command("astro build")
        elif self.static_generator == StaticGenerator.VITE:
            return self.package_manager.run_execute_command("vite build")
        elif self.static_generator == StaticGenerator.NEXT:
            return self.package_manager.run_execute_command("next export")
        elif self.static_generator == StaticGenerator.NUXT_V3:
            return self.package_manager.run_execute_command("nuxi generate")
        elif self.static_generator == StaticGenerator.NUXT_OLD:
            return self.package_manager.run_execute_command("nuxt generate")
        return False

    def build_steps(self) -> list[str]:
        output_dir = self.get_output_dir()
        get_build_command = self.get_build_command()
        install_command, install_files = self.package_manager.install_command()
        install_files = filter(lambda file: _exists(self.path, file), install_files)
        inputs_install_files = ", ".join([f'"{file}"' for file in install_files])

        return [
            'workdir(temp["build"])',
            # 'run("npx corepack enable", inputs=["package.json"], group="install")',
            f'run("{install_command}", inputs=[{inputs_install_files}], group="install")',
            'copy(".", ".", ignore=["node_modules", ".git"])',
            f'run("{get_build_command}", outputs=["{output_dir}"], group="build")',
            f'run("cp -R {output_dir}/* {{}}/".format(app["build"]))',
        ]

    def prepare_steps(self) -> Optional[list[str]]:
        return None

    def mounts(self) -> list[MountSpec]:
        return [MountSpec("temp"), *super().mounts()]

    def volumes(self) -> list[VolumeSpec]:
        return []

    def env(self) -> Optional[Dict[str, str]]:
        return None

    def services(self) -> list[ServiceSpec]:
        return []
