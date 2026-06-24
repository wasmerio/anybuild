from enum import Enum
from functools import reduce
import hashlib
import json
import os
import shlex
import subprocess
from pathlib import Path
import tomllib
from typing import Any, Dict, List, Optional, Set, TypedDict, TYPE_CHECKING

import yaml
from rich import box
from rich.panel import Panel
from rich.syntax import Syntax
from tomlkit import array, aot, comment, document, string, table

from shipit.builders.base import BuildBackend
from shipit.shipit_types import EnvStep, Package, PrepareStep, Serve, UseStep
from shipit.ui import console
from shipit.version import version as shipit_version
from shipit.volumes import load_volume_mappings, volume_mapdir_args

if TYPE_CHECKING:
    from shipit.shipit_types import Step


SHIPIT_CONFIG_ANNOTATION = "shipitcli.com/config"
SHIPIT_PROVIDER_ANNOTATION = "shipitcli.com/provider"
SHIPIT_VERSION_ANNOTATION = "shipitcli.com/version"
WASMER_APP_KIND_ANNOTATION = "wasmer.io/app-kind"


def serialize_provider_config(provider_config: Any) -> Dict[str, Any]:
    if provider_config is None:
        return {}
    if hasattr(provider_config, "model_dump"):
        return provider_config.model_dump(mode="json", exclude_none=True)
    if isinstance(provider_config, dict):
        return provider_config
    return {}


def resolve_app_kind(provider: str, framework: Any = None) -> Optional[str]:
    if provider == "wordpress":
        return "wordpress"

    if isinstance(framework, Enum):
        framework = framework.value
    if framework is None:
        return None

    provider_name = {
        "node": "javascript",
        "node-static": "javascript",
    }.get(provider, provider)
    framework_name = str(framework).lower()
    app_kinds = {
        "python": {"django", "mcp"},
        "php": {"moodle", "drupal"},
        "javascript": {"ghost", "strapi"},
    }
    if framework_name in app_kinds.get(provider_name, set()):
        return framework_name
    return None


class MapperItem(TypedDict, total=False):
    dependencies: Dict[str, str]
    scripts: Set[str]
    env: Dict[str, str]
    aliases: Dict[str, str]
    architecture_dependencies: Dict[str, Dict[str, str]]


class WasmerRunner:
    rewrite_binaries: Dict[str, str] = {
        "python": "python",
        "python3": "python",
        "python3.13": "python",
        "daphne": "python -m daphne",
        "gunicorn": "python -m gunicorn",
        "uvicorn": "python -m uvicorn",
        "alembic": "python -m alembic",
        "hypercorn": "python -m hypercorn",
        "fastapi": "python -m fastapi",
        "streamlit": "python -m streamlit",
        "next": "node node_modules/.bin/next",
        "nuxt": "node node_modules/.bin/nuxt",
        "gatsby": "node node_modules/.bin/gatsby",
        "svelte": "node node_modules/.bin/svelte",
        "remix": "node node_modules/.bin/remix",
        "astro": "node node_modules/.bin/astro",
        "vite": "node node_modules/.bin/vite",
        "hexo": "node node_modules/.bin/hexo",
        "flask": "python -m flask",
        "mcp": "python -m mcp",
        "node": "edge",
    }

    mapper: Dict[str, MapperItem] = {
        "node": {
            "dependencies": {
                "latest": "wasmer/edgejs-quickjs@=0.0.4",
                "24": "wasmer/edgejs-quickjs@=0.0.4",
                "22": "wasmer/edgejs-quickjs@=0.0.4",
            },
            "scripts": {"edge"},
            "aliases": {"node": "edge"},
            "env": {},
        },
        "python": {
            "dependencies": {
                "latest": "python/python@=3.13.5",
                "3.13": "python/python@=3.13.5",
            },
            "scripts": {"python"},
            "aliases": {},
            "env": {
                "PYTHONEXECUTABLE": "/bin/python",
            },
        },
        "pandoc": {
            "dependencies": {
                "latest": "wasmer/pandoc@=0.0.1",
                "3.5": "wasmer/pandoc@=0.0.1",
            },
            "scripts": {"pandoc"},
        },
        "ffmpeg": {
            "dependencies": {
                "latest": "wasmer/ffmpeg@=1.0.5",
                "N-111519": "wasmer/ffmpeg@=1.0.5",
            },
            "scripts": {"ffmpeg"},
        },
        "php": {
            "dependencies": {
                "latest": "php/php-32@=8.3.2102",
                "8.3": "php/php-32@=8.3.2102",
                "8.3.29": "php/php-32@=8.3.2102",
                "8.2": "php/php-32@=8.2.2801",
                "8.1": "php/php-32@=8.1.3201",
                "7.4": "php/php-32@=7.4.3301",
            },
            "architecture_dependencies": {
                "64-bit": {
                    "latest": "php/php-64@=8.3.2102",
                    "8.3": "php/php-64@=8.3.2102",
                    "8.3.29": "php/php-64@=8.3.2102",
                    "8.2": "php/php-64@=8.2.2801",
                    "8.1": "php/php-64@=8.1.3201",
                    "7.4": "php/php-64@=7.4.3301",
                },
                "32-bit": {
                    "latest": "php/php-32@=8.3.2102",
                    "8.3": "php/php-32@=8.3.2102",
                    "8.3.29": "php/php-32@=8.3.2102",
                    "8.2": "php/php-32@=8.2.2801",
                    "8.1": "php/php-32@=8.1.3201",
                    "7.4": "php/php-32@=7.4.3301",
                },
            },
            "scripts": {"php"},
            "aliases": {},
            "env": {},
        },
        "phpix": {
            "dependencies": {
                "latest": "phpix/phpix-83-32bit@=0.2.2",
                "8.3": "phpix/phpix-83-32bit@=0.2.2",
                "8.3.29": "phpix/phpix-83-32bit@=0.2.2",
                "8.2": "phpix/phpix-82-32bit@=0.2.2",
                "8.1": "phpix/phpix-81-32bit@=0.2.2",
                "7.4": "php/php-32@=7.4.3301", # Note, we don't have PHPix + PHP 7.4 and never will
            },
            "architecture_dependencies": {
                "64-bit": {
                    "latest": "phpix/phpix-83-64bit@=0.2.2",
                    "8.3": "phpix/phpix-83-64bit@=0.2.2",
                    "8.3.29": "phpix/phpix-83-64bit@=0.2.2",
                    "8.2": "phpix/phpix-82-64bit@=0.2.2",
                    "8.1": "phpix/phpix-81-64bit@=0.2.2",
                    "7.4": "php/php-64@=7.4.3301",
                },
                "32-bit": {
                    "latest": "phpix/phpix-83-32bit@=0.2.2",
                    "8.3": "phpix/phpix-83-32bit@=0.2.2",
                    "8.3.29": "phpix/phpix-83-32bit@=0.2.2",
                    "8.2": "phpix/phpix-82-32bit@=0.2.2",
                    "8.1": "phpix/phpix-81-32bit@=0.2.2",
                    "7.4": "php/php-32@=7.4.3301",
                },
            },
            "scripts": {"php", "phpix"},
            "aliases": {"php": "phpix"},
            "env": {},
        },
        "bash": {
            "dependencies": {
                "latest": "wasmer/bash@=1.0.25",
                "8.3": "wasmer/bash@=1.0.25",
            },
            "scripts": {"bash", "sh"},
            "aliases": {},
            "env": {},
        },
        "static-web-server": {
            "dependencies": {
                "latest": "wasmer/static-web-server@=1.1.0",
                "2.38.0": "wasmer/static-web-server@=1.1.0",
                "0.1": "wasmer/static-web-server@=1.1.0",
            },
            "scripts": {"webserver"},
            "aliases": {"static-web-server": "webserver"},
            "env": {},
        },
    }

    def __init__(
        self,
        build_backend: BuildBackend,
        src_dir: Path,
        registry: Optional[str] = None,
        token: Optional[str] = None,
        bin: Optional[str] = None,
        shipit_dir: Optional[Path] = None,
    ) -> None:
        self.build_backend = build_backend
        self.src_dir = src_dir
        self.shipit_dir = shipit_dir or self.src_dir / ".shipit"
        self.wasmer_dir_path = self.shipit_dir / "wasmer"
        self.wasmer_registry = registry
        self.wasmer_token = token
        self.bin = bin or "wasmer"
        self.provider_config: Any = None

    def get_serve_mount_path(self, name: str) -> Path:
        if name == "app":
            return Path("/app")
        else:
            return Path("/opt") / name

    def prepare_config(self, provider_config: Any) -> Any:
        from shipit.providers.python import PythonConfig
        from shipit.providers.php import PhpConfig
        from shipit.providers.node import NodeConfig

        if isinstance(provider_config, PythonConfig):
            provider_config.python_extra_index_url = (
                "https://pythonindex.wasix.org/simple"
            )
            provider_config.cross_platform = "wasix_wasm32"
            provider_config.precompile_python = True
        if isinstance(provider_config, PhpConfig):
            provider_config.phpix = True
        if isinstance(provider_config, NodeConfig):
            provider_config.use_edgejs = True
            provider_config.remove_native_binaries = True
        self.provider_config = provider_config
        return provider_config

    def prepare_build_steps(self, build_steps: List["Step"]) -> List["Step"]:
        new_build_steps: List["Step"] = []
        for step in build_steps:
            if isinstance(step, UseStep):
                deps = []
                found_go = False
                for package in step.dependencies:
                    if package.name == "go":
                        deps.append(Package(name="go-wasix"))
                        found_go = True
                    else:
                        deps.append(package)
                if found_go:
                    step = UseStep(dependencies=deps)
                new_build_steps.append(step)
                if found_go:
                    new_build_steps.append(
                        EnvStep(
                            variables={
                                "GOOS": "wasip1",
                                "GOARCH": "wasm",
                            }
                        )
                    )
            else:
                new_build_steps.append(step)
        return new_build_steps

    def build(self, serve: Serve) -> None:
        self.build_prepare(serve)
        self.build_serve(serve)

    def build_prepare(self, serve: Serve) -> None:
        if not serve.prepare:
            return
        prepare_dir = self.wasmer_dir_path / "prepare"
        prepare_dir.mkdir(parents=True, exist_ok=True)
        env = dict(serve.env or {})
        for dep in serve.deps:
            if dep.name in self.mapper:
                dep_env = self.mapper[dep.name].get("env")
                if dep_env is not None:
                    env.update(dep_env)
        if env:
            env_lines = [f"export {k}={v}" for k, v in env.items()]
            env_lines = "\n".join(env_lines)
        else:
            env_lines = ""

        commands: List[str] = []
        if serve.cwd:
            commands.append(f"cd {serve.cwd}")

        if serve.prepare:
            for step in serve.prepare:
                commands.append(step.command)

        body = "\n".join(filter(None, [env_lines, *commands]))
        content = f"#!/bin/bash\n\n{body}"
        console.print(
            "\n[bold]Created prepare.sh script to run before packaging ✅[/bold]"
        )
        manifest_panel = Panel(
            Syntax(
                content,
                "bash",
                theme="monokai",
                background_color="default",
                line_numbers=True,
            ),
            box=box.SQUARE,
            border_style="bright_black",
            expand=False,
        )
        console.print(manifest_panel, markup=False, highlight=True)

        (prepare_dir / "prepare.sh").write_text(content)
        (prepare_dir / "prepare.sh").chmod(0o755)

    def prepare(self, env: Dict[str, str], prepare: List[PrepareStep]) -> None:
        prepare_dir = self.wasmer_dir_path / "prepare"
        self.run_serve_command(
            "bash /prepare/prepare.sh",
            volume_mappings={
                **load_volume_mappings(self.src_dir, shipit_dir=self.shipit_dir),
                "/prepare": prepare_dir,
            },
        )

    def find_file_in_mounts(self, serve: Serve, program: str) -> Optional[Path]:
        program_path = Path(program)
        for mount in serve.mounts or []:
            if program.startswith(str(mount.serve_path.absolute())):
                module_path = program_path.relative_to(mount.serve_path).as_posix()
                full_module_path = (
                    self.build_backend.get_artifact_mount_path(mount.name) / module_path
                )
                return full_module_path
        return None

    def build_serve(self, serve: Serve) -> None:
        doc = document()
        doc.add(comment(f"Wasmer manifest generated with Shipit v{shipit_version}"))
        package = table()
        doc.add("package", package)
        package.add("entrypoint", "start")
        dependencies = table()
        doc.add("dependencies", dependencies)

        binaries: Dict[str, Dict[str, Optional[Dict[str, str]]]] = {}

        deps = serve.deps or []
        if serve.prepare and not any(dep.name == "bash" for dep in deps):
            deps.append(Package("bash"))

        if deps:
            console.print("[bold]Mapping dependencies to Wasmer packages:[/bold]")
        for dep in deps:
            if dep.name in self.mapper:
                version = dep.version or "latest"
                mapped_dependencies = self.mapper[dep.name]["dependencies"]
                if dep.architecture:
                    architecture_dependencies = (
                        self.mapper[dep.name]
                        .get("architecture_dependencies", {})
                        .get(dep.architecture, {})
                    )
                    if architecture_dependencies:
                        mapped_dependencies = architecture_dependencies
                if version in mapped_dependencies:
                    console.print(
                        f"* {dep.name}@{version} mapped to {self.mapper[dep.name]['dependencies'][version]}"
                    )
                    package_name, version = mapped_dependencies[version].split("@")
                    dependencies.add(package_name, version)
                    scripts = self.mapper[dep.name].get("scripts") or []
                    for script in scripts:
                        binaries[script] = {
                            "script": f"{package_name}:{script}",
                            "env": self.mapper[dep.name].get("env"),
                        }
                    aliases = self.mapper[dep.name].get("aliases") or {}
                    for alias, script in aliases.items():
                        binaries[alias] = {
                            "script": f"{package_name}:{script}",
                            "env": self.mapper[dep.name].get("env"),
                        }
                else:
                    raise Exception(
                        f"Dependency {dep.name}@{version} not found in Wasmer"
                    )
            else:
                raise Exception(f"Dependency {dep.name} not found in Wasmer")

        fs = table()
        doc.add("fs", fs)
        if serve.mounts:
            for mount in serve.mounts:
                fs.add(
                    str(mount.serve_path.absolute()),
                    str(
                        self.build_backend.get_artifact_mount_path(
                            mount.name
                        ).absolute()
                    ),
                )

        modules = None

        if serve.commands:
            commands = aot()
            doc.add("command", commands)
            for command_name, command_line in serve.commands.items():
                command = table()
                commands.append(command)
                parts = shlex.split(command_line)
                program = parts[0]
                command_env = {}
                command_module = None
                if program in self.rewrite_binaries:
                    rewritten_program = shlex.split(self.rewrite_binaries[program])
                    program = rewritten_program[0]
                    parts[:1] = rewritten_program
                elif program not in binaries:
                    if program.startswith("/"):
                        # It's an executable
                        full_module_path = self.find_file_in_mounts(serve, program)
                        if not full_module_path:
                            raise Exception(
                                f"Could not find {program} in the mounts or in the binaries list"
                            )
                        # Check if is a WebAssembly module by checking the contents
                        if not full_module_path.read_bytes().startswith(b"\0asm"):
                            raise Exception(
                                f"Wasmer can only execute WebAssembly modules, binary {program} is not."
                            )

                        if not modules:
                            modules = aot()
                            doc.add("module", modules)

                        module = table()
                        modules.append(module)
                        module_name = program.replace("/", "_").lower().lstrip("_")
                        module.add("name", module_name)
                        module.add("source", str(full_module_path.absolute()))

                        command_module = module_name
                    else:
                        raise Exception(f"Binary {program} not runable in Wasmer yet")

                if not command_module:
                    program_binary = binaries.get(program)
                    if not program_binary:
                        raise Exception(
                            f"Command {command_name} is trying to run {program} but it is not available (dependencies: {', '.join(dependencies.keys())}, programs: {', '.join(binaries.keys())})"
                        )
                    command_module = program_binary["script"]
                    command_env.update(program_binary.get("env") or {})

                command.add("name", command_name)
                command.add("module", command_module)
                command.add("runner", "wasi")
                wasi_args = table()
                if serve.cwd:
                    wasi_args.add("cwd", serve.cwd)
                wasi_args.add("main-args", array(parts[1:]).multiline(True))
                if serve.env:
                    command_env.update(serve.env)
                if command_env:
                    arr = array([f"{k}={v}" for k, v in command_env.items()]).multiline(
                        True
                    )
                    wasi_args.add("env", arr)
                title = string("annotations.wasi", literal=False)
                command.add(title, wasi_args)

        self.wasmer_dir_path.mkdir(parents=True, exist_ok=True)

        manifest = doc.as_string().replace(
            '[command."annotations.wasi"]', "[command.annotations.wasi]"
        )
        console.print("\n[bold]Created wasmer.toml manifest ✅[/bold]")
        manifest_panel = Panel(
            Syntax(
                manifest.strip(),
                "toml",
                theme="monokai",
                background_color="default",
                line_numbers=True,
            ),
            box=box.SQUARE,
            border_style="bright_black",
            expand=False,
        )
        console.print(manifest_panel, markup=False, highlight=True)
        (self.wasmer_dir_path / "wasmer.toml").write_text(manifest)

        original_app_yaml_path = self.src_dir / "app.yaml"
        if original_app_yaml_path.exists():
            console.print(
                "[bold]Using original app.yaml found in source directory[/bold]"
            )
            yaml_config = yaml.safe_load(original_app_yaml_path.read_text())
        else:
            yaml_config = {
                "kind": "wasmer.io/App.v0",
            }
        yaml_config["package"] = "."
        if serve.services:
            capabilities = yaml_config.get("capabilities", {})
            assert isinstance(capabilities, dict), "capabilities must be a dictionary"
            has_mysql = any(service.provider == "mysql" for service in serve.services)
            if has_mysql:
                capabilities["database"] = {"engine": "mysql"}
            yaml_config["capabilities"] = capabilities

        if serve.volumes:
            volumes_yaml = yaml_config.get("volumes", [])
            assert isinstance(volumes_yaml, list), "volumes must be a list"
            for vol in serve.volumes:
                mount_path = str(vol.serve_path)
                existing_volume = next(
                    (
                        volume_yaml
                        for volume_yaml in volumes_yaml
                        if isinstance(volume_yaml, dict)
                        and volume_yaml.get("mount") == mount_path
                    ),
                    None,
                )
                if existing_volume is not None:
                    existing_volume["name"] = vol.name
                else:
                    volumes_yaml.append(
                        {
                            "name": vol.name,
                            "mount": mount_path,
                        }
                    )
            yaml_config["volumes"] = volumes_yaml

        has_php = any(dep.name == "php" for dep in serve.deps)
        has_phpix = any(dep.name == "phpix" for dep in serve.deps)
        build_deps = reduce(
            lambda acc, dep: acc + dep.dependencies,
            [step for step in serve.build if isinstance(step, UseStep)],
            [],
        )
        is_built_with_go = any(dep.name in ["go", "go-wasix"] for dep in build_deps)

        # Note: phpix is multi-threaded, so this is only needed for raw php server and go.
        if (has_php and not has_phpix) or is_built_with_go:
            scaling = yaml_config.get("scaling", {})
            assert isinstance(scaling, dict), "scaling must be a dictionary"
            scaling["mode"] = "single_concurrency"
            yaml_config["scaling"] = scaling

        if has_phpix and serve.provider == "wordpress":
            capabilities = yaml_config.get("capabilities", {})
            assert isinstance(capabilities, dict), "capabilities must be a dictionary"
            memory = capabilities.get("memory", {})
            assert isinstance(memory, dict), "memory must be a dictionary"
            memory["limit"] = "2Gb"
            capabilities["memory"] = memory
            yaml_config["capabilities"] = capabilities
            yaml_config["enable_email"] = True

        if has_phpix and serve.env and serve.env.get("PHPIX_PHP_THREADS"):
            app_env = yaml_config.get("env", {})
            assert isinstance(app_env, dict), "env must be a dictionary"
            app_env["PHPIX_PHP_THREADS"] = str(serve.env["PHPIX_PHP_THREADS"])
            yaml_config["env"] = app_env

        if "after_deploy" in serve.commands:
            jobs = yaml_config.get("jobs", [])
            # Filter out any existing after_deploy job
            jobs = [job for job in jobs if job.get("name") != "after_deploy"]
            jobs.append(
                {
                    "name": "after_deploy",
                    "trigger": "post-deployment",
                    "action": {"execute": {"command": "after_deploy"}},
                }
            )
            yaml_config["jobs"] = jobs

        annotations = yaml_config.get("annotations", {})
        assert isinstance(annotations, dict), "annotations must be a dictionary"
        annotations[SHIPIT_CONFIG_ANNOTATION] = serialize_provider_config(
            self.provider_config
        )
        annotations[SHIPIT_VERSION_ANNOTATION] = shipit_version
        annotations[SHIPIT_PROVIDER_ANNOTATION] = serve.provider
        app_kind = resolve_app_kind(
            serve.provider,
            getattr(self.provider_config, "framework", None),
        )
        if app_kind:
            annotations[WASMER_APP_KIND_ANNOTATION] = app_kind
        yaml_config["annotations"] = annotations

        app_yaml = yaml.dump(yaml_config)

        console.print("\n[bold]Created app.yaml manifest ✅[/bold]")
        app_yaml_panel = Panel(
            Syntax(
                app_yaml.strip(),
                "yaml",
                theme="monokai",
                background_color="default",
                line_numbers=True,
            ),
            box=box.SQUARE,
            border_style="bright_black",
            expand=False,
        )
        console.print(app_yaml_panel, markup=False, highlight=True)
        (self.wasmer_dir_path / "app.yaml").write_text(app_yaml)

    def run_serve_command(
        self,
        command: str,
        volume_mappings: Optional[Dict[str, str]] = None,
        env: Optional[Dict[str, str]] = None,
    ) -> None:
        parsed_command = shlex.split(command)
        if not parsed_command:
            raise ValueError("Serve command cannot be empty")

        command_name = parsed_command[0]
        command_args = parsed_command[1:]
        extra_args = []
        run_args = ["run"]

        # if self.command_uses_edgejs(command_name):
        #     run_args.append("--experimental-napi")

        if self.wasmer_registry:
            extra_args = [f"--registry={self.wasmer_registry}"] + extra_args
        run_env = {**os.environ, **(env or {})}
        self.run_command(
            self.bin,
            [
                *run_args,
                str(self.wasmer_dir_path.absolute()),
                "--net",
                "--forward-host-env",
                f"--command={command_name}",
                *volume_mapdir_args(
                    self.build_backend,
                    volume_mappings or {},
                ),
                *extra_args,
                *(["--", *command_args] if command_args else []),
            ],
            env=run_env,
        )

    def command_uses_edgejs(self, command: str) -> bool:
        wasmer_toml_path = self.wasmer_dir_path / "wasmer.toml"
        if not wasmer_toml_path.exists():
            return False

        manifest = tomllib.loads(wasmer_toml_path.read_text())
        commands = manifest.get("command", [])
        if not isinstance(commands, list):
            return False

        for item in commands:
            if not isinstance(item, dict) or item.get("name") != command:
                continue
            module = str(item.get("module", ""))
            annotations = item.get("annotations", {})
            wasi = {}
            if isinstance(annotations, dict):
                wasi = annotations.get("wasi", {})
            atom = wasi.get("atom") if isinstance(wasi, dict) else None
            return "edgejs" in module or atom == "edgejs"
        return False

    def has_serve_command(self, command: str) -> bool:
        wasmer_toml_path = self.wasmer_dir_path / "wasmer.toml"
        if not wasmer_toml_path.exists():
            return False

        manifest = tomllib.loads(wasmer_toml_path.read_text())
        commands = manifest.get("command", [])
        if not isinstance(commands, list):
            return False

        return any(
            isinstance(item, dict) and item.get("name") == command
            for item in commands
        )

    def run_command(
        self,
        command: str,
        extra_args: Optional[List[str]] | None = None,
        env: Optional[Dict[str, str]] = None,
    ) -> Any:
        return subprocess.run(
            [command, *(extra_args or [])],
            check=True,
            env=env or os.environ,
        )

    def _update_app_yaml(
        self, app_owner: Optional[str] = None, app_name: Optional[str] = None
    ) -> None:
        if not app_owner and not app_name:
            return
        app_yaml_path = self.wasmer_dir_path / "app.yaml"
        if not app_yaml_path.exists():
            return
        yaml_config = yaml.safe_load(app_yaml_path.read_text()) or {}
        changed = False
        if app_owner and yaml_config.get("owner") != app_owner:
            yaml_config["owner"] = app_owner
            changed = True
        if app_name and yaml_config.get("name") != app_name:
            yaml_config["name"] = app_name
            changed = True
        if changed:
            app_yaml_path.write_text(yaml.dump(yaml_config))

    def _deploy_args(
        self, app_owner: Optional[str] = None, app_name: Optional[str] = None
    ) -> List[str]:
        extra_args: List[str] = []
        if self.wasmer_registry:
            extra_args += ["--registry", self.wasmer_registry]
        if self.wasmer_token:
            extra_args += ["--token", self.wasmer_token]
        if app_owner:
            extra_args += ["--owner", app_owner]
        if app_name:
            extra_args += ["--app-name", app_name]
        return [
            "deploy",
            "--publish-package",
            "--dir",
            self.wasmer_dir_path,
            "--non-interactive",
            *extra_args,
        ]

    def deploy_config(self, config_path: Path) -> None:
        package_webc_path = self.wasmer_dir_path / "package.webc"
        app_yaml_path = self.wasmer_dir_path / "app.yaml"
        package_webc_path.parent.mkdir(parents=True, exist_ok=True)
        self.run_command(
            self.bin,
            ["package", "build", self.wasmer_dir_path, "--out", package_webc_path],
        )
        config_path.write_text(
            json.dumps(
                {
                    "app_yaml_path": str(app_yaml_path.absolute()),
                    "package_webc_path": str(package_webc_path.absolute()),
                    "package_webc_size": package_webc_path.stat().st_size,
                    "package_webc_sha256": hashlib.sha256(
                        package_webc_path.read_bytes()
                    ).hexdigest(),
                }
            )
        )
        console.print(f"\n[bold]Saved deploy config to {config_path}[/bold]")

    def deploy(
        self, app_owner: Optional[str] = None, app_name: Optional[str] = None
    ) -> None:
        self._update_app_yaml(app_owner, app_name)
        self.run_command(self.bin, self._deploy_args(app_owner=app_owner, app_name=app_name))
