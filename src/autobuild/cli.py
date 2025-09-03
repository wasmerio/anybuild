import starlark as sl
from autobuild import Package, Serve
import os
from typing import List, Dict, Union, Optional
import logging
from shutil import copy, copytree, ignore_patterns
import sh
import shlex
import typer
from pathlib import Path
from rich.rule import Rule
from rich.panel import Panel
from rich import box
from rich.syntax import Syntax
from rich.console import Console
from autobuild.version import version as autobuild_version
import shutil

console = Console()

app = typer.Typer(invoke_without_command=True)

DIR_PATH = Path(os.path.dirname(os.path.realpath(__file__)))
ASSETS_PATH = DIR_PATH / "assets"

class RunStep:
    def __init__(
        self,
        command: str,
        inputs: List[str] = None,
        outputs: List[str] = None,
        group: str = None,
    ):
        self.command = command
        self.inputs = inputs
        self.outputs = outputs
        self.group = group


class CopyStep:
    def __init__(self, source: str, target: str, ignore: List[str] = None):
        self.source = source
        self.target = target
        self.ignore = ignore


class EnvStep:
    def __init__(self, variables: Dict[str, str]):
        self.variables = variables

    def __str__(self):
        return " ".join([f"{key}={value}" for key, value in self.variables.items()])


class UseStep:
    def __init__(self, dependencies: List[Package]):
        self.dependencies = dependencies

    # def __str__(self):
    #     return " ".join([f"{dependency.name}@{dependency.version}" for dependency in self.dependencies])


class PathStep:
    def __init__(self, path: str):
        self.path = path


Step = Union[RunStep, CopyStep, EnvStep, PathStep, UseStep]


class Build:
    def __init__(self, deps: List[Package], steps: List[Step]):
        self.deps = deps
        self.steps = steps


import sys


def write_stdout(line):
    sys.stdout.write(f"{line}")  # print to console


def write_stderr(line):
    sys.stderr.write(f"{line}")  # print to console


class DockerBuilder:
    env = {
        "HOME": "/root",
    }

    def getenv(self, name: str) -> Optional[str]:
        return self.env.get(name) or os.environ.get(name)

    def mkdir(self, src_dir: Path, path: Path):
        path = Path("/autobuilds") / path
        # path.mkdir(parents=True, exist_ok=True)
        return path.absolute()

    def build(self, src_dir: Path, env: Dict[str, str], steps: List[Step]):
        console.print(f"\n[bold]Building Docker file[/bold]")
        base_path = src_dir / ".autobuilds" / "docker"
        shutil.rmtree(base_path, ignore_errors=True)
        self.mkdir(src_dir, base_path)
        docker_file = base_path / "Dockerfile"
        docker_file_contents = "FROM debian:bookworm-slim\n"
        docker_file_contents += """
RUN apt-get update  \
    && apt-get -y --no-install-recommends install sudo curl ca-certificates locate git zip unzip \
    && rm -rf /var/lib/apt/lists/*


SHELL ["/bin/bash", "-o", "pipefail", "-c"]

RUN curl https://pkgx.sh | sh
"""
        #docker_file_contents += "RUN curl https://mise.run | sh\n"
        docker_file_contents += "RUN curl https://get.wasmer.io -sSfL | sh\n"
        docker_file_contents += "WORKDIR /app\n"
        for step in steps:
            if isinstance(step, RunStep):
                pre = "\\\n  " + "".join([f" --mount=type=bind,source={input},target={input} \\\n  " for input in step.inputs])
                docker_file_contents += f"RUN {pre} {step.command}\n"
            elif isinstance(step, CopyStep):
                docker_file_contents += f"COPY {step.source} {step.target}\n"
            elif isinstance(step, EnvStep):
                env_vars = " ".join([f"{key}={value}" for key, value in step.variables.items()])
                docker_file_contents += f"ENV {env_vars}\n"
            elif isinstance(step, PathStep):
                docker_file_contents += f"ENV PATH={step.path}:$PATH\n"
            elif isinstance(step, UseStep):
                for dependency in step.dependencies:
                    if dependency.version:
                        docker_file_contents += f"RUN pkgm install {dependency.name}@{dependency.version}\n"
                    else:
                        docker_file_contents += f"RUN pkgm install {dependency.name}\n"
                # docker_file_contents += f"RUN apt-get update && apt-get install -y {step.dependencies}\n"
        manifest_panel = Panel(Syntax(docker_file_contents.strip(), "dockerfile", theme="monokai", background_color="default", line_numbers=True), box=box.SQUARE, border_style="bright_black", expand=False)
        console.print(manifest_panel, markup=False, highlight=True)

        docker_file.write_text(docker_file_contents)

        docker_file_ignore = base_path / "Dockerfile.dockerignore"
        docker_file_ignore.write_text("""
.autobuilds
.autobuild
""")

        console.print(Rule(characters="-", style="bright_black"))
        console.print(f"[bold]Build complete ✅[/bold]")

    def build_assets(self, src_dir: Path, assets: Dict[str, str]):
        pass
        # for asset in assets:
        #     asset_path = src_dir / ".autobuilds" / "local" / "assets" / asset
        #     asset_path.parent.mkdir(parents=True, exist_ok=True)
        #     asset_path.write_text(assets[asset])

    def prepare(self, src_dir: Path, env: Dict[str, str], prepare: str):
        pass

    def buildserve(self, src_dir: Path, serve: Serve):
        sh.Command("docker")("build", "-f", (src_dir / ".autobuilds" / "docker" / "Dockerfile").absolute(), "-t", serve.name, ".", _cwd=src_dir.absolute(), _out=write_stdout, _err=write_stderr)

    def serve_mount(self, src_dir: Path, name: str):
        pass

    def get_asset(self, src_dir: Path, name: str):
        pass

    def run_serve_command(self, src_dir: Path, command: str):
        pass


class LocalBuilder:
    def __init__(self, src_dir: Path):
        self.src_dir = src_dir
    
    def execute_step(
        self, step: Step, src_dir: Path, env: Dict[str, str], build_path: Path
    ):
        if isinstance(step, UseStep):
            console.print(f"[bold]Using dependencies:[/bold] {step.dependencies}")
        elif isinstance(step, RunStep):
            extra = ""
            if step.inputs:
                for input in step.inputs:
                    copy((src_dir / input), (build_path / input))
                all_inputs = ", ".join(step.inputs)
                extra = f" [bright_black]# using {all_inputs}[/bright_black]"
            console.print(
                f"[bright_black]$[/bright_black] [bold]{step.command}[/bold]{extra}"
            )
            command_line = step.command
            parts = shlex.split(command_line)
            program = parts[0]
            extended_paths = [str(build_path / path) for path in env['PATH'].split(os.pathsep)]
            extended_paths.append(os.environ['PATH'])
            PATH = os.pathsep.join(extended_paths) # type: ignore
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
            ignore_matches.append(".autobuilds")
            ignore_matches.append(".autobuild")
            copytree(
                (src_dir / step.source),
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

    def build(self, src_dir: Path, env: Dict[str, str], steps: List[Step]):
        console.print(f"\n[bold]Building package[/bold]")
        base_path = src_dir / ".autobuilds" / "local"
        shutil.rmtree(base_path, ignore_errors=True)
        base_path.mkdir(parents=True, exist_ok=True)
        temp_path = base_path / "build"
        temp_path.mkdir(exist_ok=False)
        logging.info(f"Initialized temporary build path: {temp_path}")
        for step in steps:
            console.print(Rule(characters="-", style="bright_black"))
            self.execute_step(step, src_dir, env, temp_path)
        
        if "PATH" in env:
            path = base_path / ".path"
            path.write_text(env["PATH"]) # type: ignore

        console.print(Rule(characters="-", style="bright_black"))
        console.print(f"[bold]Build complete ✅[/bold]")
    
    def mkdir(self, src_dir: Path, path: Path):
        path = src_dir / ".autobuilds" / "local" / path
        path.mkdir(parents=True, exist_ok=True)
        return path.absolute()
    
    def create_file(self, path: Path, content: str, mode: int = 0o755):
        path.write_text(content)
        path.chmod(mode)
        return path.absolute()

    def getenv(self, name: str) -> Optional[str]:
        return os.environ.get(name)

    def build_assets(self, src_dir: Path, assets: Dict[str, str]):
        for asset in assets:
            asset_path = Path("assets") / asset
            path = self.mkdir(src_dir, asset_path.parent)
            self.create_file(path, assets[asset])

    def prepare(self, src_dir: Path, env: Dict[str, str], prepare: str):
        app_dir = src_dir / ".autobuilds" / "local" / "build"
        prepare_bash_script = src_dir / ".autobuilds" / "local" / "prepare" / "prepare.sh"
        prepare_bash_script.parent.mkdir(parents=True, exist_ok=True)
        prepare_bash_script.write_text(f"#!/bin/bash\ncd {app_dir}\n{prepare}")
        prepare_bash_script.chmod(0o755)
        sh.Command(f"{prepare_bash_script.absolute()}")(_out=write_stdout, _err=write_stderr)

    def buildserve(self, src_dir: Path, serve: Serve):
        console.print("\n[bold]Building serve[/bold]")
        base_path = src_dir / ".autobuilds" / "local"
        base_path.mkdir(exist_ok=True)
        build_path = base_path / "build"
        serve_command_path = base_path / "serve" / "bin"
        serve_command_path.mkdir(parents=True, exist_ok=False)
        path = base_path / ".path"
        # path_resolved = [str((build_path/path).resolve()) for path in path.read_text().split(os.pathsep) if path]
        # path_text = os.pathsep.join(path_resolved)
        path_text = path.read_text()
        console.print(f"[bold]Serve Commands:[/bold]")
        for command in serve.commands:
            console.print(f"* {command}")
            command_path = serve_command_path / command
            command_path.write_text(f"#!/bin/bash\ncd {build_path}\nPATH={path_text}:$PATH {serve.commands[command]}")
            command_path.chmod(0o755)

    def run_serve_command(self, src_dir: Path, command: str):
        console.print(f"\n[bold]Running {command} command[/bold]")
        base_path = src_dir / ".autobuilds" / "local" / "serve" / "bin"
        command_path = base_path / command
        sh.Command(str(command_path))(_out=write_stdout, _err=write_stderr)

    def serve_mount(self, src_dir: Path, name: str) -> str:
        base_path = src_dir / ".autobuilds" / "local" / "serve" / "mounts" / name
        base_path.mkdir(parents=True, exist_ok=True)
        return str(base_path.absolute())

    def get_asset(self, src_dir: Path, name: str):
        asset_path = ASSETS_PATH / name
        return asset_path.read_text()


class WasmerBuilder:
    mapper = {
        "python": {
            "dependencies": {
                "latest": "wasmer/python-native@=0.1.11",
                "3.13": "wasmer/python-native@=0.1.11"
            },
            "scripts": set(["python"]),
            "env": {
                "PYTHONEXECUTABLE": "/bin/python",
                "PYTHONHOME": "/cpython",
                "HOME": "/app",
            }
        },
        "php": {
            "dependencies": {
                "latest": "php/php-32@=8.3.2104",
                "8.3": "php/php-32@=8.3.2104"
            },
            "scripts": set(["php"]),
            "env": {
            }
        },
        "bash": {
            "dependencies": {
                "latest": "wasmer/bash@=1.0.24",
                "8.3": "wasmer/bash@=1.0.24"
            },
            "scripts": set(["bash", "sh"]),
            "env": {
            }
        },
        "static-web-server": {
            "dependencies": {
                "latest": "wasmer/static-web-server@=1.1.0",
                "0.1": "wasmer/static-web-server@=1.1.0"
            },
            "scripts": set(["webserver"]),
            "env": {
            }
        }
    }
    def __init__(self, inner_builder: LocalBuilder):
        self.inner_builder = inner_builder
        self.default_env = {
            "AUTOBUILD_PYTHON_EXTRA_INDEX_URL": "https://pythonindex.wasix.org/simple",
            "AUTOBUILD_PYTHON_CROSS_PLATFORM": "wasix_wasm32",
            "AUTOBUILD_WASMER_REGISTRY": "https://registry.wasmer.wtf/",
        }
    
    def getenv(self, name: str) -> Optional[str]:
        return self.inner_builder.getenv(name) or self.default_env.get(name)
    
    def build(self, src_dir: Path, env: Dict[str, str], build: List[Step]):
        return self.inner_builder.build(src_dir, env, build)

    def build_assets(self, src_dir: Path, assets: Dict[str, str]):
        return self.inner_builder.build_assets(src_dir, assets)

    def prepare(self, src_dir: Path, env: Dict[str, str], prepare: str):
        prepare_dir = self.inner_builder.mkdir(src_dir, Path("wasmer") / "prepare")
        self.inner_builder.create_file(Path(prepare_dir) / "prepare.sh", f"#!/bin/bash\ncd /app\n{prepare}", mode=0o755)
        self.run_serve_command(src_dir, "bash", extra_args=[f"--mapdir=/prepare:{prepare_dir}", "--", "/prepare/prepare.sh"])

    def buildserve(self, src_dir: Path, serve: Serve):
        from tomlkit import comment,document,nl,table, aot,string
        doc = document()
        doc.add(comment(f"File generated by Autobuild {autobuild_version}"))
        package = table()
        doc.add("package", package)
        package.add("entrypoint", "start")
        dependencies = table()
        doc.add("dependencies", dependencies)

        binaries = {}
        
        if serve.deps:
            console.print(f"[bold]Mapping dependencies:[/bold]")
        for dep in serve.deps:
            if dep.name in self.mapper:
                version = dep.version or "latest"
                if version in self.mapper[dep.name]["dependencies"]:
                    console.print(f"* {dep.name}@{version} mapped to {self.mapper[dep.name]['dependencies'][version]}")
                    package, version = self.mapper[dep.name]["dependencies"][version].split("@") # type:ignore
                    dependencies.add(package, version)
                    for script in self.mapper[dep.name]["scripts"]:
                        binaries[script] = f"{package}:{script}"
                else:
                    raise Exception(f"Dependency {dep.name}@{version} not found in Wasmer")
            else:
                raise Exception(f"Dependency {dep.name} not found in Wasmer")

        fs = table()
        doc.add("fs", fs)
        if serve.assets:
            fs.add("/assets", str((src_dir / ".autobuilds" / "local" / "assets").absolute()))
        fs.add("/app", str((src_dir / ".autobuilds" / "local" / "build").absolute()))
        if serve.mounts:
            for mount in serve.mounts:
                fs.add(mount, serve.mounts[mount])
        
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
                command.add("module", binaries[program])
                command.add("runner", "wasi")
                wasi_args = table()
                wasi_args.add("cwd", "/app")
                wasi_args.add("main-args", parts[1:])
                if program == "python":
                    wasi_args.add("env", ["PYTHONEXECUTABLE=/bin/python", "PYTHONHOME=/cpython", "HOME=/app"])
                title = string("annotations.wasi", literal=False)
                command.add(title, wasi_args)
        
        wasmer_dir = self.inner_builder.mkdir(src_dir, Path("wasmer"))

        manifest = doc.as_string().replace("[command.\"annotations.wasi\"]", "[command.annotations.wasi]")
        console.print(f"\n[bold]Created wasmer.toml manifest ✅[/bold]")
        manifest_panel = Panel(Syntax(manifest.strip(), "toml", theme="monokai", background_color="default", line_numbers=True), box=box.SQUARE, border_style="bright_black", expand=False)
        console.print(manifest_panel, markup=False, highlight=True)
        self.inner_builder.create_file(Path(wasmer_dir) / "wasmer.toml", manifest)
    
    def run_serve_command(self, src_dir: Path, command: str, extra_args: Optional[List[str]] = None):
        console.print(f"\n[bold]Serving site[/bold]: running {command} command")
        wasmer_path = self.inner_builder.mkdir(src_dir, Path("wasmer"))
        extra_args = extra_args or []
        wasmer_registry = self.getenv("AUTOBUILD_WASMER_REGISTRY")
        if wasmer_registry:
            extra_args = [f"--registry={wasmer_registry}"] + extra_args
        sh.Command("wasmer")("run", str(wasmer_path), "--net", f"--command={command}", *extra_args, _out=write_stdout, _err=write_stderr)
    
    def serve_mount(self, src_dir: Path, name: str):
        return self.inner_builder.serve_mount(src_dir, name)

    def get_asset(self, src_dir: Path, name: str):
        return self.inner_builder.get_asset(src_dir, name)

class Builder:
    def __init__(self, src_dir: Path, builder: LocalBuilder):
        self.src_dir = src_dir
        self.builder = builder

    def buildandserve(self, env: Dict[str, str], serve: Serve):
        self.builder.build(self.src_dir, env, serve.build)
        if serve.assets:
            self.builder.build_assets(self.src_dir, serve.assets)
        self.builder.buildserve(self.src_dir, serve)
        if serve.prepare:
            self.builder.prepare(self.src_dir, env, serve.prepare)
    
    def getenv(self, name: str) -> Optional[str]:
        return self.builder.getenv(name)
    
    def run_serve_command(self, command: str):
        self.builder.run_serve_command(self.src_dir, command)

    def serve_mount(self, name: str) -> str:
        return self.builder.serve_mount(self.src_dir, name)

    def get_asset(self, name: str):
        return self.builder.get_asset(self.src_dir, name)


class Ctx:
    def __init__(self, builder: Builder):
        self.builder = builder
        self.packages = {}
        self.builds = []
        self.steps = []
        self.serves = {}

    def add_package(self, package: Package):
        index = f"{package.name}@{package.version}" if package.version else package.name
        self.packages[index] = package
        return f"ref:package:{index}"

    def get_ref(self, index: str) -> Package:
        if index.startswith("ref:package:"):
            return self.packages[index[len("ref:package:") :]]
        elif index.startswith("ref:build:"):
            return self.builds[int(index[len("ref:build:") :])]
        elif index.startswith("ref:serve:"):
            return self.serves[index[len("ref:serve:") :]]
        elif index.startswith("ref:step:"):
            return self.steps[int(index[len("ref:step:") :])]
        else:
            raise Exception(f"Invalid reference: {index}")

    def get_refs(self, indices: List[str]) -> List[object]:
        return [self.get_ref(index) for index in indices if index is not None]

    def add_build(self, build: Build):
        self.builds.append(build)
        return f"ref:build:{len(self.builds) - 1}"

    def add_serve(self, serve: Serve):
        self.serves[serve.name] = serve
        return f"ref:serve:{serve.name}"

    def add_step(self, step: Step):
        if step is None:
            return None
        self.steps.append(step)
        return f"ref:step:{len(self.steps) - 1}"

    def getenv(self, name):
        return self.builder.getenv(name)

    def get_asset(self, name: str):
        return self.builder.get_asset(name)

    def dep(self, name, version=None):
        package = Package(name, version)
        return self.add_package(package)

    def serve(
        self,
        name: str,
        provider: str,
        build: List[str],
        deps: List[str],
        commands: Dict[str, str],
        assets: Dict[str, Union[str, bytes]] = None,
        prepare: str = None,
        workers: List[str] = None,
        mounts: Dict[str, str] = None,
    ) -> str:
        serve = Serve(
            name=name,
            provider=provider,
            build=self.get_refs(build),
            assets=assets,
            deps=self.get_refs(deps),
            commands=commands,
            prepare=prepare,
            workers=workers,
            mounts=mounts,
        )
        return self.add_serve(serve)

    def path(self, path: str):
        step = PathStep(path)
        return self.add_step(step)
    
    def use(self, *dependencies: List[str]):
        step = UseStep(self.get_refs(dependencies)) # type: ignore
        return self.add_step(step)

    def run(self, *args, **kwargs):
        step = RunStep(*args, **kwargs)
        return self.add_step(step)

    def copy(
        self, source: str, target: str, ignore: List[str] = None
    ):
        step = CopyStep(source, target, ignore)
        return self.add_step(step)

    def buildpath(self, name):
        return f"file://{name}"

    def env(self, **env_vars):
        step = EnvStep(env_vars)
        return self.add_step(step)

    def serve_mount(self, name):
        print(f"serve_mount called with {name}")
        return self.builder.serve_mount(name)


def print_help():
    panel = Panel(
        f"Autobuild {autobuild_version}",
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
    docker: bool = typer.Option(
        False,
        help="Use Docker to build the project.",
    ),
    start: bool = typer.Option(
        False,
        help="Run the start command after building.",
    ),
):
    if not (path / ".autobuild").exists():
        generate(path)

    build(path, wasmer=wasmer, docker=docker)
    if start:
        serve(path, wasmer=wasmer, docker=docker)
    # deploy(path)


@app.command(name="generate")
def generate(
    path: Path = typer.Argument(
        Path("."),
        help="Project path (defaults to current directory).",
        show_default=False,
    ),
):
    raise NotImplementedError("Autobuild generation is not yet implemented")


@app.callback(
    invoke_without_command=True,
    context_settings={"allow_extra_args": True, "ignore_unknown_options": True},
)
def _default(ctx: typer.Context):
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
    docker: bool = typer.Option(
        False,
        help="Use Docker to build the project.",
    ),
    start: Optional[bool] = typer.Option(
        True,
        help="Run the start command after building.",
    ),
) -> None:
    if wasmer:
        builder = Builder(path, WasmerBuilder(LocalBuilder()))
    elif docker:
        builder = Builder(path, DockerBuilder())
    else:
        builder = Builder(path, LocalBuilder())
    if start:
        builder.run_serve_command("start")

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
    docker: bool = typer.Option(
        False,
        help="Use Docker to build the project.",
    ),
) -> None:
    ab_file = path / ".autobuild"
    if not ab_file.exists():
        raise FileNotFoundError(f"Autobuild file not found at {ab_file}")
    source = open(ab_file).read()
    if wasmer:
        build = WasmerBuilder(LocalBuilder())
    elif docker:
        build = DockerBuilder()
    else:
        build = LocalBuilder()
    builder = Builder(path, build)

    ctx = Ctx(builder)
    glb = sl.Globals.standard()
    mod = sl.Module()

    mod.add_callable("getenv", ctx.getenv)
    mod.add_callable("dep", ctx.dep)
    mod.add_callable("serve", ctx.serve)
    mod.add_callable("run", ctx.run)
    mod.add_callable("use", ctx.run)
    mod.add_callable("copy", ctx.copy)
    mod.add_callable("path", ctx.path)
    mod.add_callable("buildpath", ctx.buildpath)
    mod.add_callable("get_asset", ctx.get_asset)
    mod.add_callable("env", ctx.env)
    mod.add_callable("use", ctx.use)
    mod.add_callable("serve_mount", ctx.serve_mount)

    dialect = sl.Dialect.extended()
    dialect.enable_f_strings = True

    ast = sl.parse("autobuild", source, dialect=dialect)

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
    builder.buildandserve(env, next(iter(ctx.serves.values())))


class Deployer:
    def __init__(self, executor: LocalBuilder):
        self.executor = executor

    def deploy(self, deploy_dir: Path, deploy: Serve):
        pass


def main():
    args = sys.argv[1:]
    # If no subcommand or first token looks like option/path → default to "build"
    available_commands = [cmd.name for cmd in app.registered_commands]
    if not args or args[0].startswith("-") or args[0] not in available_commands:
        sys.argv = [sys.argv[0], "auto", *args]

    try:
        app()
    except Exception as e:
        console.print(f"[bold red]{type(e).__name__}[/bold red]: {e}")
        # raise e


if __name__ == "__main__":
    main()
