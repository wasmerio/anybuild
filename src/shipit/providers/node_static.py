import json
import re
import shlex
from pathlib import Path
from typing import Any, Dict, Optional

from pydantic import model_validator
from pydantic_settings import SettingsConfigDict

from .base import (
    Config,
    DetectResult,
    DependencySpec,
    MountSpec,
    ServiceSpec,
    VolumeSpec,
    _exists,
)
from .node import NodeConfig, NodeFramework, NodeProvider, PackageManager
from .staticfile import StaticFileProvider, StaticFileConfig


class NodeStaticConfig(NodeConfig, StaticFileConfig):
    model_config = SettingsConfigDict(extra="ignore", env_prefix="SHIPIT_")

    @model_validator(mode="after")
    def validate_static_framework(self) -> "NodeStaticConfig":
        if self.framework and not self.framework.can_be_static():
            raise ValueError(
                f"{self.framework.value} cannot be generated as a static "
                "Node app"
            )
        return self


class NodeStaticProvider(NodeProvider, StaticFileProvider):
    only_build: bool = False
    SCRIPT_BUILD_COMMAND = ("build",)
    # Only use this commands if the build command is not found in the package.json
    SCRIPT_BUILD_COMMAND_FALLBACK = ("generate", "export", "docs:build",)
    _ASSEMBLE_DEST_PATTERN = re.compile(r"\bdest\s*:\s*['\"]([^'\"]+)['\"]")

    def __init__(
        self, path: Path, config: NodeStaticConfig, only_build: bool = False
    ):
        NodeProvider.__init__(self, path, config, only_build=only_build)

    @classmethod
    def load_config(
        cls, path: Path, base_config: Config
    ) -> NodeStaticConfig:
        static_config = StaticFileProvider.load_config(path, base_config)
        config_data = base_config.model_dump() | static_config.model_dump()
        config = NodeStaticConfig(**config_data)
        if not config.package_manager:
            config.package_manager = NodeProvider.detect_package_manager(path)

        package_json = cls.parse_package_json(path)

        if not config.framework:
            if cls.has_dependency(package_json, "@11ty/eleventy"):
                config.framework = NodeFramework.ELEVENTY
            elif cls.has_dependency(package_json, "vitepress"):
                config.framework = NodeFramework.VITEPRESS
            elif cls.has_dependency(package_json, "vuepress"):
                config.framework = NodeFramework.VUEPRESS
            elif cls.has_dependency(package_json, "hexo") or cls.has_dependency(
                package_json, "hexo-cli"
            ):
                config.framework = NodeFramework.HEXO
            elif cls.has_dependency(package_json, "metalsmith"):
                config.framework = NodeFramework.METALSMITH
            elif cls.has_dependency(package_json, "assemble") or cls.has_dependency(
                package_json, "grunt-assemble"
            ):
                config.framework = NodeFramework.ASSEMBLE
            elif cls.has_dependency(package_json, "harp"):
                config.framework = NodeFramework.HARP
            elif cls.has_dependency(package_json, "gatsby"):
                config.framework = NodeFramework.GATSBY
            elif cls.has_dependency(package_json, "astro"):
                config.framework = NodeFramework.ASTRO
            elif cls.has_dependency(package_json, "docusaurus"):
                config.framework = NodeFramework.DOCUSAURUS_OLD
            elif cls.has_dependency(package_json, "@docusaurus/core"):
                config.framework = NodeFramework.DOCUSAURUS
            elif cls.has_dependency(package_json, "svelte"):
                config.framework = NodeFramework.SVELTE
            elif cls.has_dependency(
                package_json, "@remix-run/dev", "1"
            ) or cls.has_dependency(package_json, "@remix-run/dev", "0"):
                config.framework = NodeFramework.REMIX_OLD
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
                    config.framework = NodeFramework.REMIX_V2
                else:
                    config.framework = NodeFramework.REMIX_V2_CLASSIC
            elif cls.has_dependency(package_json, "vite"):
                config.framework = NodeFramework.VITE
            elif cls.has_dependency(package_json, "next"):
                config.framework = NodeFramework.NEXT
            elif cls.has_dependency(package_json, "nuxt", "2") or cls.has_dependency(
                package_json, "nuxt", "1"
            ):
                config.framework = NodeFramework.NUXT_OLD
            elif cls.has_dependency(package_json, "nuxt"):
                config.framework = NodeFramework.NUXT_V3

        if not config.build_command:
            config.build_command = cls.get_build_command(
                package_json, config.package_manager, config.framework
            )

        if not config.static_dir:
            if config.framework:
                config.static_dir = cls.get_static_dir(
                    path, package_json, config.framework
                )
            else:
                config.static_dir = "dist"

        return config

    @classmethod
    def get_static_dir(
        cls,
        path: Path,
        package_json: Optional[Dict[str, Any]],
        framework: NodeFramework,
    ) -> str:
        if framework == NodeFramework.VITEPRESS:
            root = cls._script_build_root(package_json, "vitepress")
            root = root or cls._default_docs_root(path, ".vitepress")
            return cls._rooted_output_dir(root, ".vitepress/dist")

        if framework == NodeFramework.VUEPRESS:
            root = cls._script_build_root(package_json, "vuepress")
            root = root or cls._default_docs_root(path, ".vuepress")
            return cls._rooted_output_dir(root, ".vuepress/dist")

        if framework == NodeFramework.METALSMITH:
            return (
                cls._metalsmith_output_dir(path)
                or framework.get_static_output_dir()
            )

        if framework == NodeFramework.ASSEMBLE:
            return (
                cls._assemble_output_dir(path)
                or framework.get_static_output_dir()
            )

        if framework == NodeFramework.HARP:
            return (
                cls._harp_output_dir(package_json)
                or framework.get_static_output_dir()
            )

        return framework.get_static_output_dir()

    @classmethod
    def _script_commands(
        cls, package_json: Optional[Dict[str, Any]]
    ) -> list[str]:
        build_script_commands = NodeProvider._script_commands(
            package_json, preferred=cls.SCRIPT_BUILD_COMMAND
        )
        if not build_script_commands:
            # Only use these commands if the build command is not found in the package.json
            build_script_commands = NodeProvider._script_commands(
                package_json, preferred=cls.SCRIPT_BUILD_COMMAND_FALLBACK
            )
        return build_script_commands

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
    def name(cls) -> str:
        return "node-static"

    @classmethod
    def detect(
        cls, path: Path, config: Config
    ) -> Optional[DetectResult]:
        # if config.commands.install:
        #     # Detect this provider from the install command
        #     install_commands = [
        #         "npm install",
        #         "npm ci",
        #         "npm i",
        #         "pnpm install",
        #         "pnpm ci",
        #         "pnpm i",
        #         "yarn install",
        #         "yarn ci",
        #         "yarn i",
        #         "bun install",
        #         "bun ci",
        #         "bun i",
        #     ]
        #     if config.commands.install in install_commands:
        #         return DetectResult(cls.name(), 40)

        has_package_manager_build_command = False
        if config.commands.build:
            # Iterate over all generators and check if the build command matches
            for framework in NodeFramework:
                if not framework.can_be_static():
                    continue
                if framework.build_static_command() in config.commands.build:
                    return DetectResult(cls.name(), 60)

            frameworks = NodeFramework.detect_from_command(
                config.commands.build
            )
            if frameworks and NodeFramework.NEXT not in frameworks:
                return DetectResult(cls.name(), 60)

            has_package_manager_build_command = (
                cls._is_package_manager_build_command(config.commands.build)
            )

        package_json = cls.parse_package_json(path)
        # if not package_json:
        #     if has_package_manager_build_command:
        #         return DetectResult(cls.name(), 40)
        #     return None

        dependencies_that_require_full_node = [
            "@astrojs/node",
            "@remix-run/node",
            "@sveltejs/adapter-node",
        ]
        if any(cls.has_dependency(package_json, dep) for dep in dependencies_that_require_full_node):
            return None

        for build_command in cls._script_commands(package_json):
            for framework in NodeFramework:
                if not framework.can_be_static():
                    continue
                if framework.build_static_command() in build_command:
                    return DetectResult(cls.name(), 60)
            all_frameworks = NodeFramework.detect_from_command(build_command)
            if all_frameworks and all(
                framework.is_pure_static() for framework in all_frameworks
            ):
                return DetectResult(cls.name(), 60)

        pure_static_dependencies = [
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
        static_dependencies = [
            "astro",
            "vite",
            "next",
            "nuxt",
            "gatsby",
            "svelte",
            "@remix-run/dev",
        ]
        if any(cls.has_dependency(package_json, dep) for dep in pure_static_dependencies):
            return DetectResult(cls.name(), 60)
        if any(cls.has_dependency(package_json, dep) for dep in static_dependencies):
            return DetectResult(cls.name(), 20)
        if has_package_manager_build_command:
            return DetectResult(cls.name(), 20)
        return None

    def dependencies(self) -> list[DependencySpec]:
        node_provider = NodeProvider(self.path, self.config, only_build=True)
        return [
            *node_provider.dependencies(),
            *(StaticFileProvider.dependencies(self) if not self.only_build else []),
        ]

    @classmethod
    def get_build_command(
        cls,
        package_json: Optional[Dict[str, Any]],
        package_manager: PackageManager,
        framework: Optional[NodeFramework],
    ) -> Optional[str]:
        if package_json:
            scripts = package_json.get("scripts", {})
            if not isinstance(scripts, dict):
                scripts = {}
            docs_build_command = scripts.get("docs:build")
            if (
                framework
                in [NodeFramework.VITEPRESS, NodeFramework.VUEPRESS]
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
        if not framework:
            return None
        command = framework.build_static_command()
        return package_manager.run_execute_command(command)

    def build_steps(self) -> list[str]:
        return filter(
            None,
            [
                'workdir(temp.path)' if not self.only_build else None,
                *self.build_steps_install(),
                self.build_steps_copy(),
                *self.build_steps_build(output="config.static_dir"),
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
        return [
            MountSpec("temp", attach_to_serve=False),
            *StaticFileProvider.mounts(self),
        ]

    def volumes(self) -> list[VolumeSpec]:
        return []

    def env(self) -> Optional[Dict[str, str]]:
        return None

    def commands(self) -> Dict[str, str]:
        return StaticFileProvider.commands(self)

    def services(self) -> list[ServiceSpec]:
        return []
