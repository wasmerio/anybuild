import hashlib
import json
import os
import shlex
from pathlib import Path
from typing import Any, Dict, List, Optional, Set, TypedDict, cast

import sh  # type: ignore[import-untyped]
import yaml
from rich import box
from rich.panel import Panel
from rich.syntax import Syntax
from tomlkit import array, aot, comment, document, nl, string, table

from shipit.builders.base import BuildBackend
from shipit.runners.base import Runner
from shipit.shipit_types import Package, PrepareStep, RunStep, Serve, WorkdirStep
from shipit.ui import console, write_stderr, write_stdout
from shipit.version import version as shipit_version


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
        "hypercorn": "python -m hypercorn",
        "fastapi": "python -m fastapi",
        "streamlit": "python -m streamlit",
        "flask": "python -m flask",
        "mcp": "python -m mcp",
    }

    mapper: Dict[str, MapperItem] = {
        "python": {
            "dependencies": {
                "latest": "python/python@=3.13.3",
                "3.13": "python/python@=3.13.3",
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
                "8.2": "php/php-32@=8.2.2801",
                "8.1": "php/php-32@=8.1.3201",
                "7.4": "php/php-32@=7.4.3301",
            },
            "architecture_dependencies": {
                "64-bit": {
                    "latest": "php/php-64@=8.3.2102",
                    "8.3": "php/php-64@=8.3.2102",
                    "8.2": "php/php-64@=8.2.2801",
                    "8.1": "php/php-64@=8.1.3201",
                    "7.4": "php/php-64@=7.4.3301",
                },
                "32-bit": {
                    "latest": "php/php-32@=8.3.2102",
                    "8.3": "php/php-32@=8.3.2102",
                    "8.2": "php/php-32@=8.2.2801",
                    "8.1": "php/php-32@=8.1.3201",
                    "7.4": "php/php-32@=7.4.3301",
                },
            },
            "scripts": {"php"},
            "aliases": {},
            "env": {},
        },
        "bash": {
            "dependencies": {
                "latest": "wasmer/bash@=1.0.24",
                "8.3": "wasmer/bash@=1.0.24",
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
    ) -> None:
        self.build_backend = build_backend
        self.src_dir = src_dir
        self.wasmer_dir_path = self.src_dir / ".shipit" / "wasmer"
        self.wasmer_registry = registry
        self.wasmer_token = token
        self.bin = bin or "wasmer"

    def get_serve_mount_path(self, name: str) -> Path:
        if name == "app":
            return Path("/app")
        else:
            return Path("/opt") / name

    def prepare_config(self, provider_config: Any) -> Any:
        from shipit.providers.python import PythonConfig

        if isinstance(provider_config, PythonConfig):
            provider_config.python_extra_index_url = (
                "https://pythonindex.wasix.org/simple"
            )
            provider_config.cross_platform = "wasix_wasm32"
            provider_config.precompile_python = True
        return provider_config

    def build(self, serve: Serve) -> None:
        self.build_prepare(serve)
        self.build_serve(serve)

    def build_prepare(self, serve: Serve) -> None:
        if not serve.prepare:
            return
        prepare_dir = self.wasmer_dir_path / "prepare"
        prepare_dir.mkdir(parents=True, exist_ok=True)
        env = serve.env or {}
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
            f"\n[bold]Created prepare.sh script to run before packaging ✅[/bold]"
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
            "bash",
            extra_args=[
                f"--mapdir=/prepare:{prepare_dir}",
                "--",
                "/prepare/prepare.sh",
            ],
        )

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
            console.print(f"[bold]Mapping dependencies to Wasmer packages:[/bold]")
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

        doc.add(nl())
        if serve.commands:
            commands = aot()
            doc.add("command", commands)
            for command_name, command_line in serve.commands.items():
                command = table()
                commands.append(command)
                parts = shlex.split(command_line)
                program = parts[0]
                if program in self.rewrite_binaries:
                    rewritten_program = shlex.split(self.rewrite_binaries[program])
                    program = rewritten_program[0]
                    parts[:1] = rewritten_program
                elif program not in binaries:
                    raise Exception(f"Binary {program} not runable in Wasmer yet")
                command.add("name", command_name)
                program_binary = binaries.get(program)
                if not program_binary:
                    raise Exception(f"Command {command_name} is trying to run {program} but it is not available (dependencies: {', '.join(dependencies.keys())}, programs: {', '.join(binaries.keys())})")
                command.add("module", program_binary["script"])
                command.add("runner", "wasi")
                wasi_args = table()
                if serve.cwd:
                    wasi_args.add("cwd", serve.cwd)
                wasi_args.add("main-args", array(parts[1:]).multiline(True))
                env = program_binary.get("env") or {}
                if serve.env:
                    env.update(serve.env)
                if env:
                    arr = array([f"{k}={v}" for k, v in env.items()]).multiline(True)
                    wasi_args.add("env", arr)
                title = string("annotations.wasi", literal=False)
                command.add(title, wasi_args)

        self.wasmer_dir_path.mkdir(parents=True, exist_ok=True)

        manifest = doc.as_string().replace(
            '[command."annotations.wasi"]', "[command.annotations.wasi]"
        )
        console.print(f"\n[bold]Created wasmer.toml manifest ✅[/bold]")
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
                f"[bold]Using original app.yaml found in source directory[/bold]"
            )
            yaml_config = yaml.safe_load(original_app_yaml_path.read_text())
        else:
            yaml_config = {
                "kind": "wasmer.io/App.v0",
            }
        yaml_config["package"] = "."
        if serve.services:
            capabilities = yaml_config.get("capabilities", {})
            has_mysql = any(service.provider == "mysql" for service in serve.services)
            if has_mysql:
                capabilities["database"] = {"engine": "mysql"}
            yaml_config["capabilities"] = capabilities

        if serve.volumes:
            volumes_yaml = yaml_config.get("volumes", [])
            for vol in serve.volumes:
                volumes_yaml.append(
                    {
                        "name": vol.name,
                        "mount": str(vol.serve_path),
                    }
                )
            yaml_config["volumes"] = volumes_yaml

        has_php = any(dep.name == "php" for dep in serve.deps)
        if has_php:
            scaling = yaml_config.get("scaling", {})
            scaling["mode"] = "single_concurrency"
            yaml_config["scaling"] = scaling

        if "after_deploy" in serve.commands:
            jobs = yaml_config.get("jobs", [])
            jobs.append(
                {
                    "name": "after_deploy",
                    "trigger": "post-deployment",
                    "action": {"execute": {"command": "after_deploy"}},
                }
            )
            yaml_config["jobs"] = jobs

        app_yaml = yaml.dump(yaml_config)

        console.print(f"\n[bold]Created app.yaml manifest ✅[/bold]")
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
        self, command: str, extra_args: Optional[List[str]] = None
    ) -> None:
        console.print(f"\n[bold]Serving site[/bold]: running {command} command")
        extra_args = extra_args or []

        if self.wasmer_registry:
            extra_args = [f"--registry={self.wasmer_registry}"] + extra_args
        self.run_command(
            self.bin,
            [
                "run",
                str(self.wasmer_dir_path.absolute()),
                "--net",
                f"--command={command}",
                *extra_args,
            ],
        )

    def run_command(
        self, command: str, extra_args: Optional[List[str]] | None = None
    ) -> Any:
        sh.Command(command)(
            *(extra_args or []), _out=write_stdout, _err=write_stderr, _env=os.environ
        )

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
        extra_args: List[str] = []
        if self.wasmer_registry:
            extra_args += ["--registry", self.wasmer_registry]
        if self.wasmer_token:
            extra_args += ["--token", self.wasmer_token]
        if app_owner:
            extra_args += ["--owner", app_owner]
        if app_name:
            extra_args += ["--app-name", app_name]
        self.run_command(
            self.bin,
            [
                "deploy",
                "--publish-package",
                "--dir",
                self.wasmer_dir_path,
                "--non-interactive",
                *extra_args,
            ],
        )
