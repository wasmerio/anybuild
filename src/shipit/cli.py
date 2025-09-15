import logging
import os
import shlex
import shutil
import sys
import json
import yaml
from dataclasses import dataclass
from pathlib import Path
from typing import (
    Any,
    Dict,
    List,
    Optional,
    Protocol,
    Set,
    TypedDict,
    Union,
    cast,
)
from shutil import copy, copytree, ignore_patterns

import sh  # type: ignore[import-untyped]
import starlark as sl
import typer
from rich import box
from rich.console import Console
from rich.panel import Panel
from rich.rule import Rule
from rich.syntax import Syntax

from shipit.version import version as shipit_version
from shipit.generator import generate_shipit


console = Console()

app = typer.Typer(invoke_without_command=True)

DIR_PATH = Path(__file__).resolve().parent
ASSETS_PATH = DIR_PATH / "assets"


@dataclass
class Mount:
    name: str
    build_path: Path
    serve_path: Path


@dataclass
class Serve:
    name: str
    provider: str
    build: List["Step"]
    deps: List["Package"]
    commands: Dict[str, str]
    assets: Optional[Dict[str, str]] = None
    prepare: Optional[List["PrepareStep"]] = None
    workers: Optional[List[str]] = None
    mounts: Optional[List[Mount]] = None
    env: Optional[Dict[str, str]] = None


@dataclass
class Package:
    name: str
    version: Optional[str] = None

    def __str__(self) -> str:  # pragma: no cover - simple representation
        return f"{self.name}@{self.version}"


@dataclass
class RunStep:
    command: str
    inputs: Optional[List[str]] = None
    outputs: Optional[List[str]] = None
    group: Optional[str] = None


@dataclass
class WorkdirStep:
    path: Path


@dataclass
class CopyStep:
    source: str
    target: str
    ignore: Optional[List[str]] = None


@dataclass
class EnvStep:
    variables: Dict[str, str]

    def __str__(self) -> str:  # pragma: no cover - simple representation
        return " ".join([f"{key}={value}" for key, value in self.variables.items()])


@dataclass
class UseStep:
    dependencies: List[Package]


@dataclass
class PathStep:
    path: str


Step = Union[RunStep, CopyStep, EnvStep, PathStep, UseStep, WorkdirStep]
PrepareStep = Union[RunStep]


@dataclass
class Build:
    deps: List[Package]
    steps: List[Step]


def write_stdout(line: str) -> None:
    sys.stdout.write(line)  # print to console


def write_stderr(line: str) -> None:
    sys.stderr.write(line)  # print to console


class MapperItem(TypedDict):
    dependencies: Dict[str, str]
    scripts: Set[str]
    env: Dict[str, str]
    aliases: Dict[str, str]


class Builder(Protocol):
    def build(
        self, env: Dict[str, str], mounts: List[Mount], steps: List[Step]
    ) -> None: ...
    def build_assets(self, assets: Dict[str, str]) -> None: ...
    def build_prepare(self, serve: Serve) -> None: ...
    def build_serve(self, serve: Serve) -> None: ...
    def finalize_build(self, serve: Serve) -> None: ...
    def prepare(self, env: Dict[str, str], prepare: List[PrepareStep]) -> None: ...
    def getenv(self, name: str) -> Optional[str]: ...
    def run_serve_command(self, command: str) -> None: ...
    def run_command(
        self, command: str, extra_args: Optional[List[str]] | None = None
    ) -> Any: ...
    def serve_mount(self, name: str) -> str: ...
    def get_asset(self, name: str) -> str: ...
    def get_build_mount_path(self, name: str) -> Path: ...
    def get_serve_mount_path(self, name: str) -> Path: ...


class DockerBuilder:
    def __init__(self, src_dir: Path, docker_client: Optional[str] = None) -> None:
        self.src_dir = src_dir
        self.docker_file_contents = ""
        self.docker_path = self.src_dir / ".shipit" / "docker"
        self.docker_out_path = self.docker_path / "out"
        self.depot_metadata = self.docker_path / "depot-build.json"
        self.docker_file_path = self.docker_path / "Dockerfile"
        self.docker_name_path = self.docker_path / "name"
        self.docker_ignore_path = self.docker_path / "Dockerfile.dockerignore"
        self.shipit_docker_path = Path("/shipit")
        self.docker_client = docker_client or "docker"
        self.env = {
            "HOME": "/root",
        }

    def get_mount_path(self, name: str) -> Path:
        if name == "app":
            return Path("app")
        else:
            return Path("opt") / name

    def get_build_mount_path(self, name: str) -> Path:
        path = Path("/") / self.get_mount_path(name)
        return path

    def get_serve_mount_path(self, name: str) -> Path:
        return self.docker_out_path / self.get_mount_path(name)

    @property
    def is_depot(self) -> bool:
        return self.docker_client == "depot"

    def getenv(self, name: str) -> Optional[str]:
        return self.env.get(name) or os.environ.get(name)

    def mkdir(self, path: Path) -> Path:
        path = self.shipit_docker_path / path
        self.docker_file_contents += f"RUN mkdir -p {str(path.absolute())}\n"
        return path.absolute()

    def build_dockerfile(self, image_name: str) -> None:
        self.docker_file_path.write_text(self.docker_file_contents)
        self.docker_name_path.write_text(image_name)
        self.print_dockerfile()
        extra_args = []
        # if self.is_depot:
        #     # We load the docker image back into the local docker daemon
        #     # extra_args += ["--load"]
        #     extra_args += ["--save", f"--metadata-file={self.depot_metadata.absolute()}"]
        sh.Command(self.docker_client)(
            "build",
            "-f",
            (self.docker_path / "Dockerfile").absolute(),
            "-t",
            image_name,
            "--platform",
            "linux/amd64",
            "--output",
            self.docker_out_path.absolute(),
            ".",
            *extra_args,
            _cwd=self.src_dir.absolute(),
            _env=os.environ,  # Pass the current environment variables to the Docker client
            _out=write_stdout,
            _err=write_stderr,
        )
        # if self.is_depot:
        #     json_text = self.depot_metadata.read_text()
        #     json_data = json.loads(json_text)
        #     build_data = json_data["depot.build"]
        #     image_id = build_data["buildID"]
        #     project = build_data["projectID"]
        #     sh.Command("depot")(
        #         "pull",
        #         "--platform",
        #         "linux/amd64",
        #         "--project",
        #         project,
        #         image_id,
        #         _cwd=self.src_dir.absolute(),
        #         _env=os.environ,  # Pass the current environment variables to the Docker client
        #         _out=write_stdout,
        #         _err=write_stderr,
        #     )
        #     # console.print(f"[bold]Image ID:[/bold] {image_id}")

    def finalize_build(self, serve: Serve) -> None:
        console.print(f"\n[bold]Building Docker file[/bold]")
        self.build_dockerfile(serve.name)
        console.print(Rule(characters="-", style="bright_black"))
        console.print(f"[bold]Build complete ✅[/bold]")

    def run_command(self, command: str, extra_args: Optional[List[str]] = None) -> Any:
        image_name = self.docker_name_path.read_text()
        return sh.Command(
            "docker"
        )(
            "run",
            "-p",
            "80:80",
            "--rm",
            image_name,
            command,
            *(extra_args or []),
            _env=os.environ,  # Pass the current environment variables to the Docker client
            _out=write_stdout,
            _err=write_stderr,
        )

    def create_file(self, path: Path, content: str, mode: int = 0o755) -> Path:
        # docker_files = self.docker_path / "files" / path.name
        # docker_files.write_text(content)
        # docker_files.chmod(mode)
        self.docker_file_contents += f"""
RUN cat > {path.absolute()} <<'EOF'
{content}
EOF

RUN chmod {oct(mode)[2:]} {path.absolute()}
"""

        return path.absolute()

    def print_dockerfile(self) -> None:
        docker_file = self.docker_path / "Dockerfile"
        manifest_panel = Panel(
            Syntax(
                docker_file.read_text(),
                "dockerfile",
                theme="monokai",
                background_color="default",
                line_numbers=True,
            ),
            box=box.SQUARE,
            border_style="bright_black",
            expand=False,
        )
        console.print(manifest_panel, markup=False, highlight=True)

    def add_dependency(self, dependency: Package):
        if dependency.name == "pie":
            self.docker_file_contents += f"RUN apt-get update && apt-get -y --no-install-recommends install gcc make autoconf libtool bison re2c pkg-config libpq-dev\n"
            self.docker_file_contents += f"RUN curl -L --output /usr/bin/pie https://github.com/php/pie/releases/download/1.2.0/pie.phar && chmod +x /usr/bin/pie\n"
            return
        elif dependency.name == "static-web-server":
            if dependency.version:
                self.docker_file_contents += (
                    f"ENV SWS_INSTALL_VERSION={dependency.version}\n"
                )
            self.docker_file_contents += f"RUN curl --proto '=https' --tlsv1.2 -sSfL https://get.static-web-server.net | sh\n"
            return
        if dependency.version:
            self.docker_file_contents += (
                f"RUN pkgm install {dependency.name}@{dependency.version}\n"
            )
        else:
            self.docker_file_contents += f"RUN pkgm install {dependency.name}\n"

    def build(
        self, env: Dict[str, str], mounts: List[Mount], steps: List[Step]
    ) -> None:
        base_path = self.docker_path
        shutil.rmtree(base_path, ignore_errors=True)
        base_path.mkdir(parents=True, exist_ok=True)
        self.docker_file_contents = "FROM debian:bookworm-slim AS build\n"
        self.docker_file_contents += """
RUN apt-get update \\
    && apt-get -y --no-install-recommends install sudo curl ca-certificates locate git zip unzip \\
    && rm -rf /var/lib/apt/lists/*

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

RUN curl https://pkgx.sh | sh
"""
        # docker_file_contents += "RUN curl https://mise.run | sh\n"
        #         self.docker_file_contents += """
        # RUN curl https://get.wasmer.io -sSfL | sh -s "v6.1.0-rc.3"
        # ENV PATH="/root/.wasmer/bin:${PATH}"
        # """
        for mount in mounts:
            self.docker_file_contents += f"RUN mkdir -p {mount.build_path.absolute()}\n"

        for step in steps:
            if isinstance(step, WorkdirStep):
                self.docker_file_contents += f"WORKDIR {step.path.absolute()}\n"
            elif isinstance(step, RunStep):
                if step.inputs:
                    pre = "\\\n  " + "".join(
                        [
                            f"--mount=type=bind,source={input},target={input} \\\n  "
                            for input in step.inputs
                        ]
                    )
                else:
                    pre = ""
                self.docker_file_contents += f"RUN {pre}{step.command}\n"
            elif isinstance(step, CopyStep):
                self.docker_file_contents += f"COPY {step.source} {step.target}\n"
            elif isinstance(step, EnvStep):
                env_vars = " ".join(
                    [f"{key}={value}" for key, value in step.variables.items()]
                )
                self.docker_file_contents += f"ENV {env_vars}\n"
            elif isinstance(step, PathStep):
                self.docker_file_contents += f"ENV PATH={step.path}:$PATH\n"
            elif isinstance(step, UseStep):
                for dependency in step.dependencies:
                    self.add_dependency(dependency)

        self.docker_file_contents += """
FROM scratch
"""
        for mount in mounts:
            self.docker_file_contents += (
                f"COPY --from=build {mount.build_path} {mount.build_path}\n"
            )

        self.docker_ignore_path.write_text("""
.shipit
Shipit
""")

    def build_assets(self, assets: Dict[str, str]) -> None:
        raise NotImplementedError

    def get_path(self) -> Path:
        return Path("/")

    def get_build_path(self) -> Path:
        return self.get_path() / "app"

    def get_serve_path(self) -> Path:
        return self.get_path() / "serve"

    def get_assets_path(self) -> Path:
        path = self.get_path() / "assets"
        self.mkdir(path)
        return path

    def prepare(self, env: Dict[str, str], prepare: List[PrepareStep]) -> None:
        raise NotImplementedError

    def build_serve(self, serve: Serve) -> None:
        serve_command_path = self.mkdir(Path("serve") / "bin")
        console.print(f"[bold]Serve Commands:[/bold]")
        for dep in serve.deps:
            self.add_dependency(dep)

        build_path = self.get_build_path()
        for command in serve.commands:
            console.print(f"* {command}")
            command_path = serve_command_path / command
            self.create_file(
                command_path,
                f"#!/bin/bash\ncd {build_path}\n{serve.commands[command]}",
                mode=0o755,
            )

    def serve_mount(self, name: str) -> str:
        path = self.mkdir(Path("serve") / "mounts" / name)
        return str(path.absolute())

    def get_asset(self, name: str) -> str:
        asset_path = ASSETS_PATH / name
        return asset_path.read_text()

    def run_serve_command(self, command: str) -> None:
        path = self.shipit_docker_path / "serve" / "bin" / command
        self.run_command(str(path))


class LocalBuilder:
    def __init__(self, src_dir: Path) -> None:
        self.src_dir = src_dir
        self.local_path = self.src_dir / ".shipit" / "local"
        self.prepare_bash_script = self.local_path / "prepare" / "prepare.sh"
        self.build_path = self.local_path / "build"
        self.workdir = self.build_path

    def get_mount_path(self, name: str) -> Path:
        if name == "app":
            return self.build_path / "app"
        else:
            return self.build_path / "opt" / name

    def get_build_mount_path(self, name: str) -> Path:
        return self.get_mount_path(name)

    def get_serve_mount_path(self, name: str) -> Path:
        return self.get_mount_path(name)

    def execute_step(self, step: Step, env: Dict[str, str]) -> None:
        build_path = self.workdir
        if isinstance(step, UseStep):
            console.print(f"[bold]Using dependencies:[/bold] {step.dependencies}")
        elif isinstance(step, WorkdirStep):
            console.print(f"[bold]Working in {step.path}[/bold]")
            self.workdir = step.path
        elif isinstance(step, RunStep):
            extra = ""
            if step.inputs:
                for input in step.inputs:
                    print(f"Copying {input} to {build_path / input}")
                    copy((self.src_dir / input), (build_path / input))
                all_inputs = ", ".join(step.inputs)
                extra = f" [bright_black]# using {all_inputs}[/bright_black]"
            console.print(
                f"[bright_black]$[/bright_black] [bold]{step.command}[/bold]{extra}"
            )
            command_line = step.command
            parts = shlex.split(command_line)
            program = parts[0]
            extended_paths = [
                str(build_path / path) for path in env["PATH"].split(os.pathsep)
            ]
            extended_paths.append(os.environ["PATH"])
            PATH = os.pathsep.join(extended_paths)  # type: ignore
            exe = shutil.which(program, path=PATH)
            if not exe:
                raise Exception(f"Program is not installed: {program}")
            cmd = sh.Command(exe)  # "grep"
            result = cmd(
                *parts[1:],
                _env={**env, "PATH": PATH},
                _cwd=build_path,
                _out=write_stdout,
                _err=write_stderr,
            )
        elif isinstance(step, CopyStep):
            ignore_extra = ""
            if step.ignore:
                ignore_extra = (
                    f" [bright_black]# ignoring {', '.join(step.ignore)}[/bright_black]"
                )
            if step.target == ".":
                console.print(f"[bold]Copy from {step.source}[/bold]{ignore_extra}")
            else:
                console.print(
                    f"[bold]Copy to {step.target} from {step.source}[/bold]{ignore_extra}"
                )
            ignore_matches = step.ignore if step.ignore else []
            ignore_matches.append(".shipit")
            ignore_matches.append("Shipit")
            copytree(
                (self.src_dir / step.source),
                (build_path / step.target),
                dirs_exist_ok=True,
                ignore=ignore_patterns(*ignore_matches),
            )
        elif isinstance(step, EnvStep):
            print(f"Setting environment variables: {step}")
            env.update(step.variables)
        elif isinstance(step, PathStep):
            console.print(f"[bold]Add {step.path}[/bold] to PATH")
            fullpath = step.path
            env["PATH"] = f"{fullpath}{os.pathsep}{env['PATH']}"
        else:
            raise Exception(f"Unknown step type: {type(step)}")

    def build(
        self, env: Dict[str, str], mounts: List[Mount], steps: List[Step]
    ) -> None:
        console.print(f"\n[bold]Building package[/bold]")
        base_path = self.local_path
        shutil.rmtree(base_path, ignore_errors=True)
        base_path.mkdir(parents=True, exist_ok=True)
        self.build_path.mkdir(exist_ok=True)
        for mount in mounts:
            mount.build_path.mkdir(parents=True, exist_ok=True)
        for step in steps:
            console.print(Rule(characters="-", style="bright_black"))
            self.execute_step(step, env)

        if "PATH" in env:
            path = base_path / ".path"
            path.write_text(env["PATH"])  # type: ignore

        console.print(Rule(characters="-", style="bright_black"))
        console.print(f"[bold]Build complete ✅[/bold]")

    def mkdir(self, path: Path) -> Path:
        path = self.get_path() / path
        path.mkdir(parents=True, exist_ok=True)
        return path.absolute()

    def create_file(self, path: Path, content: str, mode: int = 0o755) -> Path:
        path.write_text(content)
        path.chmod(mode)
        return path.absolute()

    def run_command(self, command: str, extra_args: Optional[List[str]] = None) -> Any:
        return sh.Command(command)(
            *(extra_args or []),
            _out=write_stdout,
            _err=write_stderr,
            _env=os.environ,
        )

    def getenv(self, name: str) -> Optional[str]:
        return os.environ.get(name)

    def get_path(self) -> Path:
        return self.local_path

    def get_build_path(self) -> Path:
        return self.get_path() / "build"

    def get_serve_path(self) -> Path:
        return self.get_path() / "serve"

    def get_assets_path(self) -> Path:
        path = self.get_path() / "assets"
        self.mkdir(path)
        return path

    def build_assets(self, assets: Dict[str, str]) -> None:
        assets_path = self.get_assets_path()
        for asset in assets:
            asset_path = assets_path / asset
            self.create_file(asset_path, assets[asset])

    def build_prepare(self, serve: Serve) -> None:
        app_dir = self.get_build_path()
        self.prepare_bash_script.parent.mkdir(parents=True, exist_ok=True)
        commands: List[str] = []
        if serve.prepare:
            for step in serve.prepare:
                if isinstance(step, RunStep):
                    commands.append(step.command)
                elif isinstance(step, WorkdirStep):
                    commands.append(f"cd {step.path}")
        content = "#!/bin/bash\ncd {app_dir}\n{body}".format(
            app_dir=app_dir, body="\n".join(commands)
        )
        self.prepare_bash_script.write_text(content)
        self.prepare_bash_script.chmod(0o755)

    def finalize_build(self, serve: Serve) -> None:
        pass

    def prepare(self, env: Dict[str, str], prepare: List[PrepareStep]) -> None:
        sh.Command(f"{self.prepare_bash_script.absolute()}")(
            _out=write_stdout, _err=write_stderr
        )

    def build_serve(self, serve: Serve) -> None:
        console.print("\n[bold]Building serve[/bold]")
        build_path = self.get_build_path()
        serve_command_path = self.get_serve_path() / "bin"
        serve_command_path.mkdir(parents=True, exist_ok=False)
        path = self.get_path() / ".path"
        # path_resolved = [str((build_path/path).resolve()) for path in path.read_text().split(os.pathsep) if path]
        # path_text = os.pathsep.join(path_resolved)
        path_text = path.read_text()
        console.print(f"[bold]Serve Commands:[/bold]")
        for command in serve.commands:
            console.print(f"* {command}")
            command_path = serve_command_path / command
            command_path.write_text(
                f"#!/bin/bash\ncd {build_path}\nPATH={path_text}:$PATH {serve.commands[command]}"
            )
            command_path.chmod(0o755)

    def run_serve_command(self, command: str) -> None:
        console.print(f"\n[bold]Running {command} command[/bold]")
        base_path = self.get_serve_path() / "bin"
        command_path = base_path / command
        sh.Command(str(command_path))(_out=write_stdout, _err=write_stderr)

    def serve_mount(self, name: str) -> str:
        base_path = self.get_serve_path() / "mounts" / name
        base_path.mkdir(parents=True, exist_ok=True)
        return str(base_path.absolute())

    def get_asset(self, name: str) -> str:
        asset_path = ASSETS_PATH / name
        return asset_path.read_text()


class WasmerBuilder:
    def get_build_mount_path(self, name: str) -> Path:
        return self.inner_builder.get_build_mount_path(name)

    def get_serve_mount_path(self, name: str) -> Path:
        if name == "app":
            return Path("/app")
        else:
            return Path("/opt") / name

    mapper: Dict[str, MapperItem] = {
        "python": {
            "dependencies": {
                "latest": "wasmer/python-native@=0.1.11",
                "3.13": "wasmer/python-native@=0.1.11",
            },
            "scripts": {"python"},
            "aliases": {},
            "env": {
                "PYTHONEXECUTABLE": "/bin/python",
                "PYTHONHOME": "/cpython",
            },
        },
        "php": {
            "dependencies": {
                "latest": "php/php-32@=8.3.2104",
                "8.3": "php/php-32@=8.3.2104",
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
        inner_builder: Builder,
        src_dir: Path,
        registry: Optional[str] = None,
        token: Optional[str] = None,
        bin: Optional[Path] = None,
    ) -> None:
        self.src_dir = src_dir
        self.inner_builder = inner_builder
        # The path where we store the directory of the wasmer app in the inner builder
        self.wasmer_dir_path = self.src_dir / ".shipit" / "wasmer"
        self.wasmer_registry = registry
        self.wasmer_token = token
        self.bin = bin.absolute() if bin else "wasmer"
        self.default_env = {
            "SHIPIT_PYTHON_EXTRA_INDEX_URL": "https://pythonindex.wasix.org/simple",
            "SHIPIT_PYTHON_CROSS_PLATFORM": "wasix_wasm32",
            "SHIPIT_PYTHON_PRECOMPILE": "true",
        }

    def getenv(self, name: str) -> Optional[str]:
        return self.inner_builder.getenv(name) or self.default_env.get(name)

    def build(
        self, env: Dict[str, str], mounts: List[Mount], build: List[Step]
    ) -> None:
        return self.inner_builder.build(env, mounts, build)

    def build_assets(self, assets: Dict[str, str]) -> None:
        return self.inner_builder.build_assets(assets)

    def get_build_path(self) -> Path:
        return Path("/app")

    def build_prepare(self, serve: Serve) -> None:
        print("Building prepare")
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
        if serve.prepare:
            for step in serve.prepare:
                if isinstance(step, RunStep):
                    commands.append(step.command)
                elif isinstance(step, WorkdirStep):
                    commands.append(f"cd {step.path}")
        body = "\n".join(filter(None, [env_lines, *commands]))
        (prepare_dir / "prepare.sh").write_text(
            f"#!/bin/bash\n\n{body}",
        )
        (prepare_dir / "prepare.sh").chmod(0o755)

    def finalize_build(self, serve: Serve) -> None:
        inner = cast(Any, self.inner_builder)
        inner.finalize_build(serve)

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
        from tomlkit import comment, document, nl, table, aot, string

        doc = document()
        doc.add(comment(f"File generated by Shipit {shipit_version}"))
        package = table()
        doc.add("package", package)
        package.add("entrypoint", "start")
        dependencies = table()
        doc.add("dependencies", dependencies)

        binaries = {}

        deps = serve.deps or []
        # We add bash if it's not present, as the prepare command is run in bash
        if serve.prepare:
            if not any(dep.name == "bash" for dep in deps):
                deps.append(Package("bash"))

        if deps:
            console.print(f"[bold]Mapping dependencies:[/bold]")
        for dep in deps:
            if dep.name in self.mapper:
                version = dep.version or "latest"
                if version in self.mapper[dep.name]["dependencies"]:
                    console.print(
                        f"* {dep.name}@{version} mapped to {self.mapper[dep.name]['dependencies'][version]}"
                    )
                    package_name, version = self.mapper[dep.name]["dependencies"][
                        version
                    ].split("@")
                    dependencies.add(package_name, version)
                    for script in self.mapper[dep.name]["scripts"]:
                        binaries[script] = {
                            "script": f"{package_name}:{script}",
                            "env": self.mapper[dep.name].get("env"),
                        }
                    for alias, script in self.mapper[dep.name]["aliases"].items():
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
        inner = cast(Any, self.inner_builder)
        if serve.assets:
            fs.add("/assets", str((inner.get_path() / "assets").absolute()))
        # fs.add("/app", str(inner.get_build_path().absolute()))
        if serve.mounts:
            for mount in serve.mounts:
                fs.add(
                    str(mount.serve_path.absolute()),
                    str(self.inner_builder.get_serve_mount_path(mount.name).absolute()),
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
                command.add("name", command_name)
                program_binary = binaries[program]
                command.add("module", program_binary["script"])
                command.add("runner", "wasi")
                wasi_args = table()
                wasi_args.add("cwd", "/app")
                wasi_args.add("main-args", parts[1:])
                env = program_binary.get("env") or {}
                if serve.env:
                    env.update(serve.env)
                if env:
                    wasi_args.add(
                        "env",
                        [f"{k}={v}" for k, v in env.items()],
                    )
                title = string("annotations.wasi", literal=False)
                command.add(title, wasi_args)

        inner = cast(Any, self.inner_builder)
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
            console.print(f"[bold]Using original app.yaml found in source directory[/bold]")
            yaml_config = yaml.safe_load(original_app_yaml_path.read_text())
        else:
            yaml_config = {
                "kind": "wasmer.io/App.v0",
            }
        # Update the app to use the new package
        yaml_config["package"] = "."

        app_yaml = yaml.dump(yaml_config)
        (self.wasmer_dir_path / "app.yaml").write_text(app_yaml)

        # self.inner_builder.build_serve(serve)

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

    def serve_mount(self, name: str) -> str:
        return self.inner_builder.serve_mount(name)

    def get_asset(self, name: str) -> str:
        return self.inner_builder.get_asset(name)

    def run_command(
        self, command: str, extra_args: Optional[List[str]] | None = None
    ) -> Any:
        sh.Command(command)(
            *(extra_args or []), _out=write_stdout, _err=write_stderr, _env=os.environ
        )

    def deploy(
        self, app_owner: Optional[str] = None, app_name: Optional[str] = None
    ) -> str:
        extra_args = []
        if self.wasmer_registry:
            extra_args += ["--registry", self.wasmer_registry]
        if self.wasmer_token:
            extra_args += ["--token", self.wasmer_token]
        if app_owner:
            extra_args += ["--owner", app_owner]
        if app_name:
            extra_args += ["--app-name", app_name]
        # self.run_command(
        #     self.bin,
        #     [
        #         "package",
        #         "push",
        #         self.wasmer_dir_path,
        #         "--namespace",
        #         app_owner,
        #         "--non-interactive",
        #         *extra_args,
        #     ],
        # )
        return self.run_command(
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


class Ctx:
    def __init__(self, builder: Builder) -> None:
        self.builder = builder
        self.packages: Dict[str, Package] = {}
        self.builds: List[Build] = []
        self.steps: List[Step] = []
        self.serves: Dict[str, Serve] = {}
        self.mounts: List[Mount] = []

    def add_package(self, package: Package) -> str:
        index = f"{package.name}@{package.version}" if package.version else package.name
        self.packages[index] = package
        return f"ref:package:{index}"

    def get_ref(self, index: str) -> Any:
        if index.startswith("ref:package:"):
            return self.packages[index[len("ref:package:") :]]
        elif index.startswith("ref:build:"):
            return self.builds[int(index[len("ref:build:") :])]
        elif index.startswith("ref:serve:"):
            return self.serves[index[len("ref:serve:") :]]
        elif index.startswith("ref:step:"):
            return self.steps[int(index[len("ref:step:") :])]
        elif index.startswith("ref:mount:"):
            return self.mounts[int(index[len("ref:mount:") :])]
        else:
            raise Exception(f"Invalid reference: {index}")

    def get_refs(self, indices: List[str]) -> List[Any]:
        return [self.get_ref(index) for index in indices if index is not None]

    def add_build(self, build: Build) -> str:
        self.builds.append(build)
        return f"ref:build:{len(self.builds) - 1}"

    def add_serve(self, serve: Serve) -> str:
        self.serves[serve.name] = serve
        return f"ref:serve:{serve.name}"

    def add_step(self, step: Step) -> Optional[str]:
        if step is None:
            return None
        self.steps.append(step)
        return f"ref:step:{len(self.steps) - 1}"

    def getenv(self, name: str) -> Optional[str]:
        return self.builder.getenv(name)

    def get_asset(self, name: str) -> Optional[str]:
        return self.builder.get_asset(name)

    def dep(self, name: str, version: Optional[str] = None) -> str:
        package = Package(name, version)
        return self.add_package(package)

    def serve(
        self,
        name: str,
        provider: str,
        build: List[str],
        deps: List[str],
        commands: Dict[str, str],
        assets: Optional[Dict[str, str]] = None,
        prepare: Optional[List[str]] = None,
        workers: Optional[List[str]] = None,
        mounts: Optional[List[Mount]] = None,
        env: Optional[Dict[str, str]] = None,
    ) -> str:
        build_refs = [cast(Step, r) for r in self.get_refs(build)]
        prepare_steps: Optional[List[PrepareStep]] = None
        if prepare is not None:
            # Resolve referenced steps and keep only RunStep for prepare
            resolved = [cast(Step, r) for r in self.get_refs(prepare)]
            prepare_steps = [
                cast(RunStep, s) for s in resolved if isinstance(s, RunStep)
            ]
        dep_refs = [cast(Package, r) for r in self.get_refs(deps)]
        serve = Serve(
            name=name,
            provider=provider,
            build=build_refs,
            assets=assets,
            deps=dep_refs,
            commands=commands,
            prepare=prepare_steps,
            workers=workers,
            mounts=self.get_refs([mount["ref"] for mount in mounts])
            if mounts
            else None,
            env=env,
        )
        return self.add_serve(serve)

    def path(self, path: str) -> Optional[str]:
        step = PathStep(path)
        return self.add_step(step)

    def use(self, *dependencies: str) -> Optional[str]:
        deps = [cast(Package, r) for r in self.get_refs(list(dependencies))]
        step = UseStep(deps)
        return self.add_step(step)

    def run(self, *args: Any, **kwargs: Any) -> Optional[str]:
        step = RunStep(*args, **kwargs)
        return self.add_step(step)

    def workdir(self, path: str) -> Optional[str]:
        step = WorkdirStep(Path(path))
        return self.add_step(step)

    def copy(
        self, source: str, target: str, ignore: Optional[List[str]] = None
    ) -> Optional[str]:
        step = CopyStep(source, target, ignore)
        return self.add_step(step)

    def buildpath(self, name: str) -> str:
        return str((self.builder.get_build_path() / name).absolute())

    def env(self, **env_vars: str) -> Optional[str]:
        step = EnvStep(env_vars)
        return self.add_step(step)

    def add_mount(self, mount: Mount) -> Optional[str]:
        self.mounts.append(mount)
        return f"ref:mount:{len(self.mounts) - 1}"

    def mount(self, name: str) -> Optional[str]:
        build_path = self.builder.get_build_mount_path(name)
        serve_path = self.builder.get_serve_mount_path(name)
        mount = Mount(name, build_path, serve_path)
        ref = self.add_mount(mount)
        return {
            "ref": ref,
            "build": str(build_path.absolute()),
            "serve": str(serve_path.absolute()),
        }

    def serve_mount(self, name: str) -> Optional[str]:
        return self.builder.serve_mount(name)


def print_help() -> None:
    panel = Panel(
        f"Shipit {shipit_version}",
        box=box.ROUNDED,
        border_style="blue",
        expand=False,
    )
    console.print(panel)


@app.command(name="auto")
def auto(
    path: Path = typer.Argument(
        Path("."),
        help="Project path (defaults to current directory).",
        show_default=False,
    ),
    wasmer: bool = typer.Option(
        False,
        help="Use Wasmer to build and serve the project.",
    ),
    wasmer_bin: Optional[Path] = typer.Option(
        None,
        help="The path to the Wasmer binary.",
    ),
    docker: bool = typer.Option(
        False,
        help="Use Docker to build the project.",
    ),
    docker_client: Optional[str] = typer.Option(
        None,
        help="Use a specific Docker client (such as depot, podman, etc.)",
    ),
    skip_prepare: bool = typer.Option(
        False,
        help="Run the prepare command after building (defaults to True).",
    ),
    start: bool = typer.Option(
        False,
        help="Run the start command after building.",
    ),
    regenerate: bool = typer.Option(
        None,
        help="Regenerate the Shipit file.",
    ),
    regenerate_path: Optional[Path] = typer.Option(
        None,
        help="Regenerate the Shipit file onto the provided path.",
    ),
    wasmer_deploy: Optional[bool] = typer.Option(
        False,
        help="Deploy the project to Wasmer.",
    ),
    wasmer_token: Optional[str] = typer.Option(
        None,
        help="Wasmer token.",
    ),
    wasmer_registry: Optional[str] = typer.Option(
        None,
        help="Wasmer registry.",
    ),
    wasmer_app_owner: Optional[str] = typer.Option(
        None,
        help="Owner of the Wasmer app.",
    ),
    wasmer_app_name: Optional[str] = typer.Option(
        None,
        help="Name of the Wasmer app.",
    ),
):
    if not path.exists():
        raise Exception(f"The path {path} does not exist")

    if not (path / "Shipit").exists() or regenerate or regenerate_path is not None:
        generate(path, out=regenerate_path)

    build(
        path,
        wasmer=(wasmer or wasmer_deploy),
        docker=docker,
        docker_client=docker_client,
        wasmer_registry=wasmer_registry,
        wasmer_token=wasmer_token,
        wasmer_bin=wasmer_bin,
        skip_prepare=skip_prepare,
    )
    if start or wasmer_deploy:
        serve(
            path,
            wasmer=wasmer,
            wasmer_bin=wasmer_bin,
            docker=docker,
            docker_client=docker_client,
            start=start,
            wasmer_token=wasmer_token,
            wasmer_registry=wasmer_registry,
            wasmer_deploy=wasmer_deploy,
            wasmer_app_owner=wasmer_app_owner,
            wasmer_app_name=wasmer_app_name,
        )
    # deploy(path)


@app.command(name="generate")
def generate(
    path: Path = typer.Argument(
        Path("."),
        help="Project path (defaults to current directory).",
        show_default=False,
    ),
    out: Optional[Path] = typer.Option(
        None,
        help="Output path (defaults to the Shipit file in the provided path).",
    ),
):
    if not path.exists():
        raise Exception(f"The path {path} does not exist")

    if out is None:
        out = path / "Shipit"
    content = generate_shipit(path)
    out.write_text(content)
    console.print(f"[bold]Generated Shipit[/bold] at {out.absolute()}")


@app.callback(
    invoke_without_command=True,
    context_settings={"allow_extra_args": True, "ignore_unknown_options": True},
)
def _default(ctx: typer.Context) -> None:
    print_help()


@app.command(name="deploy")
def deploy(
    path: Path = typer.Argument(
        Path("."),
        help="Project path (defaults to current directory).",
        show_default=False,
    ),
) -> None:
    pass


@app.command(name="serve")
def serve(
    path: Path = typer.Argument(
        Path("."),
        help="Project path (defaults to current directory).",
        show_default=False,
    ),
    wasmer: bool = typer.Option(
        False,
        help="Use Wasmer to build and serve the project.",
    ),
    wasmer_bin: Optional[Path] = typer.Option(
        None,
        help="The path to the Wasmer binary.",
    ),
    docker: bool = typer.Option(
        False,
        help="Use Docker to build the project.",
    ),
    docker_client: Optional[str] = typer.Option(
        None,
        help="Use a specific Docker client (such as depot, podman, etc.)",
    ),
    start: Optional[bool] = typer.Option(
        True,
        help="Run the start command after building.",
    ),
    wasmer_deploy: Optional[bool] = typer.Option(
        False,
        help="Deploy the project to Wasmer.",
    ),
    wasmer_token: Optional[str] = typer.Option(
        None,
        help="Wasmer token.",
    ),
    wasmer_registry: Optional[str] = typer.Option(
        None,
        help="Wasmer registry.",
    ),
    wasmer_app_owner: Optional[str] = typer.Option(
        None,
        help="Owner of the Wasmer app.",
    ),
    wasmer_app_name: Optional[str] = typer.Option(
        None,
        help="Name of the Wasmer app.",
    ),
) -> None:
    if not path.exists():
        raise Exception(f"The path {path} does not exist")

    builder: Builder
    if docker or docker_client:
        builder = DockerBuilder(path, docker_client)
    else:
        builder = LocalBuilder(path)
    if wasmer or wasmer_deploy:
        builder = WasmerBuilder(
            builder, path, registry=wasmer_registry, token=wasmer_token, bin=wasmer_bin
        )
    if start:
        builder.run_serve_command("start")

    if wasmer_deploy:
        if isinstance(builder, WasmerBuilder):
            builder.deploy(app_owner=wasmer_app_owner, app_name=wasmer_app_name)
        else:
            raise Exception("Wasmer deploy is only supported for Wasmer builders")


@app.command(name="build")
def build(
    path: Path = typer.Argument(
        Path("."),
        help="Project path (defaults to current directory).",
        show_default=False,
    ),
    wasmer: bool = typer.Option(
        False,
        help="Use Wasmer to build and serve the project.",
    ),
    skip_prepare: bool = typer.Option(
        False,
        help="Run the prepare command after building (defaults to True).",
    ),
    wasmer_bin: Optional[Path] = typer.Option(
        None,
        help="The path to the Wasmer binary.",
    ),
    wasmer_registry: Optional[str] = typer.Option(
        None,
        help="Wasmer registry.",
    ),
    wasmer_token: Optional[str] = typer.Option(
        None,
        help="Wasmer token.",
    ),
    docker: bool = typer.Option(
        False,
        help="Use Docker to build the project.",
    ),
    docker_client: Optional[str] = typer.Option(
        None,
        help="Use a specific Docker client (such as depot, podman, etc.)",
    ),
) -> None:
    if not path.exists():
        raise Exception(f"The path {path} does not exist")

    ab_file = path / "Shipit"
    if not ab_file.exists():
        raise FileNotFoundError(
            f"Shipit file not found at {ab_file}. Run `shipit generate {path}` to create it."
        )
    source = open(ab_file).read()
    builder: Builder
    if docker or docker_client:
        builder = DockerBuilder(path, docker_client)
    else:
        builder = LocalBuilder(path)
    if wasmer:
        builder = WasmerBuilder(
            builder, path, registry=wasmer_registry, token=wasmer_token, bin=wasmer_bin
        )

    ctx = Ctx(builder)
    glb = sl.Globals.standard()
    mod = sl.Module()

    mod.add_callable("getenv", ctx.getenv)
    mod.add_callable("dep", ctx.dep)
    mod.add_callable("serve", ctx.serve)
    mod.add_callable("run", ctx.run)
    mod.add_callable("mount", ctx.mount)
    mod.add_callable("workdir", ctx.workdir)
    mod.add_callable("copy", ctx.copy)
    mod.add_callable("path", ctx.path)
    mod.add_callable("buildpath", ctx.buildpath)
    mod.add_callable("get_asset", ctx.get_asset)
    mod.add_callable("env", ctx.env)
    mod.add_callable("use", ctx.use)
    # REMOVE ME
    mod.add_callable("serve_mount", ctx.serve_mount)

    dialect = sl.Dialect.extended()
    dialect.enable_f_strings = True

    ast = sl.parse("shipit", source, dialect=dialect)

    sl.eval(mod, ast, glb)
    # assert len(ctx.builds) == 1, "Only one build is allowed for now"
    assert len(ctx.serves) <= 1, "Only one serve is allowed for now"
    # build = ctx.builds[0]
    env = {
        "PATH": "",
        "COLORTERM": os.environ.get("COLORTERM", ""),
        "LSCOLORS": os.environ.get("LSCOLORS", "0"),
        "LS_COLORS": os.environ.get("LS_COLORS", "0"),
        "CLICOLOR": os.environ.get("CLICOLOR", "0"),
    }
    serve = next(iter(ctx.serves.values()))

    # Build and serve
    builder.build(env, serve.mounts, serve.build)
    if serve.prepare:
        builder.build_prepare(serve)
    if serve.assets:
        builder.build_assets(serve.assets)
    builder.build_serve(serve)
    builder.finalize_build(serve)
    if serve.prepare and not skip_prepare:
        builder.prepare(env, serve.prepare)


def main() -> None:
    args = sys.argv[1:]
    # If no subcommand or first token looks like option/path → default to "build"
    available_commands = [cmd.name for cmd in app.registered_commands]
    if not args or args[0].startswith("-") or args[0] not in available_commands:
        sys.argv = [sys.argv[0], "auto", *args]

    try:
        app()
    except Exception as e:
        console.print(f"[bold red]{type(e).__name__}[/bold red]: {e}")
        raise e


if __name__ == "__main__":
    main()
