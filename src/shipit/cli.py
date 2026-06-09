import tempfile
import os
import sys
import json
import re
from pathlib import Path
from dataclasses import dataclass
from typing import Any, Dict, List, Optional, Tuple, cast, Literal

import xingque as sl
import typer
from rich import box
from rich.panel import Panel
from rich.syntax import Syntax

from shipit.generator import generate_shipit, load_provider, load_provider_config
from shipit.providers.base import Config
from dotenv import dotenv_values
from shipit.builders import BuildBackend, DockerBuildBackend, LocalBuildBackend
from shipit.runners import Runner, LocalRunner, WasmerRunner
from shipit.shipit_types import (
    Build,
    CopyStep,
    EnvStep,
    Mount,
    Package,
    PathStep,
    PrepareStep,
    RunStep,
    Serve,
    Service,
    Step,
    UseStep,
    Volume,
    WriteFileStep,
    WorkdirStep,
)
from shipit.ui import console
from shipit.version import version as shipit_version
from shipit.volumes import (
    build_volumes,
    load_volume_mappings,
    merge_volume_mappings,
    parse_cli_volume_mappings,
)

app = typer.Typer(invoke_without_command=True)

DIR_PATH = Path(__file__).resolve().parent
ASSETS_PATH = DIR_PATH / "assets"
OPTIONAL_RUN_COMMANDS = {"start", "after_deploy"}


@dataclass
class CtxMount:
    ref: str
    path: str
    serve_path: str


@dataclass(frozen=True)
class ProjectPaths:
    workspace_root: Path
    app_path: Path
    subdir: Optional[str] = None


def resolve_project_paths(path: Path, subdir: Optional[Path] = None) -> ProjectPaths:
    workspace_root = path.resolve()
    if subdir is None:
        return ProjectPaths(workspace_root, workspace_root)

    if subdir.is_absolute():
        raise ValueError("--subdir must be relative to the project path")

    subdir_text = subdir.as_posix().strip("/")
    if not subdir_text or subdir_text == ".":
        return ProjectPaths(workspace_root, workspace_root)

    app_path = (workspace_root / subdir_text).resolve()
    try:
        app_path.relative_to(workspace_root)
    except ValueError:
        raise ValueError("--subdir must stay inside the project path") from None

    if not app_path.exists():
        raise ValueError(f"--subdir does not exist: {subdir_text}")
    if not app_path.is_dir():
        raise ValueError(f"--subdir is not a directory: {subdir_text}")

    normalized_subdir = app_path.relative_to(workspace_root).as_posix()
    return ProjectPaths(workspace_root, app_path, normalized_subdir)


def read_shipit_subdir(shipit_file: Path) -> Optional[Path]:
    if not shipit_file.exists():
        return None
    match = re.search(
        r"^app_subdir\s*=\s*(\"(?:\\.|[^\"])*\")\s*$",
        shipit_file.read_text(),
        re.MULTILINE,
    )
    if not match:
        return None
    value = json.loads(match.group(1))
    return Path(value) if value else None


def load_env_files(
    project_paths: ProjectPaths,
    env_name: Optional[str],
    target: Dict[str, str],
) -> None:
    env_dirs = [project_paths.workspace_root]
    if project_paths.subdir:
        env_dirs.append(project_paths.app_path)

    env_names = [".env"]
    if env_name:
        env_names.append(f".env.{env_name}")

    for env_dir in env_dirs:
        for env_file_name in env_names:
            env_file = env_dir / env_file_name
            if env_file.exists():
                target.update(dotenv_values(env_file))


def _rewrite_package_manager_command(
    command: Optional[str],
    old_manager: Any,
    new_manager: Any,
) -> Optional[str]:
    if not command:
        return command
    old_run = f"{old_manager.value} run "
    if command.startswith(old_run):
        return f"{new_manager.value} run {command[len(old_run):]}"
    return command


def apply_subdir_workspace_config(
    project_paths: ProjectPaths,
    provider_config: Any,
) -> None:
    if not project_paths.subdir or not hasattr(provider_config, "package_manager"):
        return
    if not (project_paths.app_path / "package.json").exists():
        return

    from shipit.providers.node import NodeProvider, PackageManager

    app_has_lockfile = any(
        (project_paths.app_path / manager.lockfile()).exists()
        for manager in PackageManager
    )
    if app_has_lockfile:
        return

    workspace_manager = NodeProvider.detect_package_manager(
        project_paths.workspace_root
    )
    current_manager = provider_config.package_manager
    if current_manager == workspace_manager:
        return

    provider_config.package_manager = workspace_manager
    provider_config.build_command = _rewrite_package_manager_command(
        getattr(provider_config, "build_command", None),
        current_manager,
        workspace_manager,
    )
    if provider_config.commands:
        provider_config.commands.build = _rewrite_package_manager_command(
            provider_config.commands.build,
            current_manager,
            workspace_manager,
        )


def apply_subdir_provider_config(
    project_paths: ProjectPaths,
    provider_config: Any,
) -> None:
    if hasattr(provider_config, "app_subdir"):
        provider_config.app_subdir = project_paths.subdir


class Ctx:
    def __init__(self, build_backend: BuildBackend, runner: Runner) -> None:
        self.build_backend = build_backend
        self.runner = runner
        self.packages: Dict[str, Package] = {}
        self.builds: List[Build] = []
        self.steps: List[Step] = []
        self.serves: Dict[str, Serve] = {}
        self.mounts: List[Mount] = []
        self.volumes: List[Volume] = []
        self.services: Dict[str, Service] = {}

    def add_package(self, package: Package) -> str:
        index = f"{package.name}@{package.version}" if package.version else package.name
        self.packages[index] = package
        return f"ref:package:{index}"

    def add_service(self, service: Service) -> str:
        self.services[service.name] = service
        return f"ref:service:{service.name}"

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
        elif index.startswith("ref:volume:"):
            return self.volumes[int(index[len("ref:volume:") :])]
        elif index.startswith("ref:service:"):
            return self.services[index[len("ref:service:") :]]
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

    def dep(
        self,
        name: str,
        version: Optional[str] = None,
        architecture: Optional[Literal["64-bit", "32-bit"]] = None,
    ) -> str:
        package = Package(name, version, architecture)
        return self.add_package(package)

    def service(
        self, name: str, provider: Literal["postgres", "mysql", "redis"]
    ) -> str:
        service = Service(name, provider)
        return self.add_service(service)

    def serve(
        self,
        name: str,
        provider: str,
        build: List[str],
        deps: List[str],
        commands: Dict[str, str],
        cwd: Optional[str] = None,
        prepare: Optional[List[str]] = None,
        workers: Optional[List[str]] = None,
        mounts: Optional[List[Mount]] = None,
        volumes: Optional[List[Volume]] = None,
        env: Optional[Dict[str, str]] = None,
        services: Optional[List[str]] = None,
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
            cwd=cwd,
            deps=dep_refs,
            commands=commands,
            prepare=prepare_steps,
            workers=workers,
            mounts=self.get_refs([mount.ref for mount in mounts]) if mounts else None,
            volumes=self.get_refs([volume["ref"] for volume in volumes])
            if volumes
            else None,
            env=env,
            services=self.get_refs(services) if services else None,
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
        self,
        source: str,
        target: Optional[str] = None,
        ignore: Optional[List[str]] = None,
        base: Optional[Literal["source", "assets"]] = None,
    ) -> Optional[str]:
        if target is None:
            target = source
        step = CopyStep(source, target, ignore, base or "source")
        return self.add_step(step)

    def env(self, **env_vars: str) -> Optional[str]:
        step = EnvStep(env_vars)
        return self.add_step(step)

    def write(self, path: str, content: str) -> Optional[str]:
        step = WriteFileStep(path, content)
        return self.add_step(step)

    def add_mount(self, mount: Mount) -> Optional[str]:
        self.mounts.append(mount)
        return f"ref:mount:{len(self.mounts) - 1}"

    def mount(self, name: str) -> Optional[str]:
        build_path = self.build_backend.get_build_mount_path(name)
        serve_path = self.runner.get_serve_mount_path(name)
        mount = Mount(name, build_path, serve_path)
        ref = self.add_mount(mount)

        return CtxMount(
            ref=ref,
            path=str(build_path.absolute()),
            serve_path=str(serve_path.absolute()),
        )

    def add_volume(self, volume: Volume) -> Optional[str]:
        self.volumes.append(volume)
        return f"ref:volume:{len(self.volumes) - 1}"

    def volume(self, name: str, serve: str) -> Optional[str]:
        volume = Volume(
            name=name,
            path=self.build_backend.get_volume_path(name),
            serve_path=Path(serve),
        )
        ref = self.add_volume(volume)
        return {
            "ref": ref,
            "name": name,
            "path": str(volume.path.absolute()),
            "serve": str(volume.serve_path),
            "serve_path": str(volume.serve_path),
        }


def evaluate_shipit(
    shipit_file: Path,
    build_backend: BuildBackend,
    runner: Runner,
    provider_config: Config,
) -> Tuple[Ctx, Serve]:
    source = shipit_file.read_text()
    ctx = Ctx(build_backend, runner)
    glb = sl.GlobalsBuilder.standard()

    glb.set("PORT", str(provider_config.port or "8080"))
    glb.set("config", provider_config)
    glb.set("service", ctx.service)
    glb.set("dep", ctx.dep)
    glb.set("serve", ctx.serve)
    glb.set("run", ctx.run)
    glb.set("mount", ctx.mount)
    glb.set("volume", ctx.volume)
    glb.set("workdir", ctx.workdir)
    glb.set("copy", ctx.copy)
    glb.set("write", ctx.write)
    glb.set("path", ctx.path)
    glb.set("env", ctx.env)
    glb.set("use", ctx.use)

    dialect = sl.Dialect(enable_keyword_only_arguments=True, enable_f_strings=True)

    ast = sl.AstModule.parse("Shipit", source, dialect=dialect)

    evaluator = sl.Evaluator()
    evaluator.eval_module(ast, glb.build())
    if not ctx.serves:
        raise ValueError(f"No serve definition found in {shipit_file}")
    assert len(ctx.serves) <= 1, "Only one serve is allowed for now"
    serve = next(iter(ctx.serves.values()))

    # Now we apply the custom commands (start, after_deploy, build, install)
    if provider_config.commands.start:
        serve.commands["start"] = provider_config.commands.start

    if provider_config.commands.after_deploy:
        serve.commands["after_deploy"] = provider_config.commands.after_deploy

    if provider_config.commands.build or provider_config.commands.install:
        new_build = []
        has_done_build = False
        has_done_install = False
        for step in serve.build:
            if isinstance(step, RunStep):
                if (
                    step.group == "build"
                    and not has_done_build
                    and provider_config.commands.build
                ):
                    new_build.append(
                        RunStep(provider_config.commands.build, group="build")
                    )
                    has_done_build = True
                elif (
                    step.group == "install"
                    and not has_done_install
                    and provider_config.commands.install
                ):
                    new_build.append(
                        RunStep(provider_config.commands.install, group="install")
                    )
                    has_done_install = True
                else:
                    new_build.append(step)
            else:
                new_build.append(step)
        if not has_done_install and provider_config.commands.install:
            new_build.append(RunStep(provider_config.commands.install, group="install"))
        if not has_done_build and provider_config.commands.build:
            new_build.append(RunStep(provider_config.commands.build, group="build"))
        serve.build = new_build

    if serve.commands.get("start"):
        serve.commands["start"] = serve.commands["start"].replace(
            "$PORT", str(provider_config.port or "8080")
        )

    if serve.commands.get("after_deploy"):
        serve.commands["after_deploy"] = serve.commands["after_deploy"].replace(
            "$PORT", str(provider_config.port or "8080")
        )

    return ctx, serve


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
    subdir: Optional[Path] = typer.Option(
        None,
        "--subdir",
        help="App subdirectory relative to the project path.",
    ),
    wasmer: bool = typer.Option(
        False,
        help="Use Wasmer to build and run the project.",
    ),
    wasmer_bin: Optional[str] = typer.Option(
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
    docker_opts: Optional[str] = typer.Option(
        None,
        help="Additional options to pass to the Docker client.",
    ),
    skip_docker_if_safe_build: Optional[bool] = typer.Option(
        True,
        help="Skip Docker if the build can be done safely locally (only copy commands).",
    ),
    skip_prepare: bool = typer.Option(
        False,
        help="Run the prepare command after building (defaults to True).",
    ),
    command_names: Optional[List[str]] = typer.Option(
        None,
        "-c",
        "--command",
        help="Run one or more commands after building. Can be passed multiple times.",
    ),
    volume_specs: Optional[List[str]] = typer.Option(
        None,
        "--volume",
        help="Attach one or more volumes as NAME:/guest/path. Can be passed multiple times.",
    ),
    start: bool = typer.Option(
        False,
        "--start/--no-start",
        help="Equivalent to `--command=start`.",
    ),
    after_deploy: bool = typer.Option(
        False,
        "--after-deploy/--no-after-deploy",
        help="Equivalent to `--command=after_deploy`.",
    ),
    regenerate: bool = typer.Option(
        None,
        help="Regenerate the Shipit file.",
    ),
    shipit_path: Optional[Path] = typer.Option(
        None,
        help="The path to the Shipit file (defaults to Shipit in the provided path).",
    ),
    temp_shipit: bool = typer.Option(
        False,
        help="Use a temporary Shipit file in the system temporary directory.",
    ),
    wasmer_deploy: Optional[bool] = typer.Option(
        False,
        help="Deploy the project to Wasmer.",
    ),
    wasmer_deploy_config: Optional[Path] = typer.Option(
        None,
        help="Save the output of the Wasmer build to a json file",
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
    install_command: Optional[str] = typer.Option(
        None,
        help="The install command to use (overwrites the default)",
    ),
    build_command: Optional[str] = typer.Option(
        None,
        help="The build command to use (overwrites the default)",
    ),
    start_command: Optional[str] = typer.Option(
        None,
        help="The start command to use (overwrites the default)",
    ),
    env_name: Optional[str] = typer.Option(
        None,
        help="The environment to use (defaults to `.env`, it will use .env.<env_name> if provided)",
    ),
    provider: Optional[str] = typer.Option(
        None,
        help="Use a specific provider to build the project.",
    ),
    config: Optional[str] = typer.Option(
        None,
        help="The JSON content to use as input.",
    ),
    serve_port: Optional[int] = typer.Option(
        None,
        help="The port to use (defaults to 8080).",
    ),
):
    # We assume wasmer as an active flag if we pass wasmer deploy or wasmer deploy config
    wasmer = wasmer or wasmer_deploy or (wasmer_deploy_config is not None)

    if not path.exists():
        raise Exception(f"The path {path} does not exist")
    project_paths = resolve_project_paths(path, subdir)

    if temp_shipit:
        if shipit_path:
            raise Exception("Cannot use both --temp-shipit and --shipit-path")
        temp_shipit = tempfile.NamedTemporaryFile(
            delete=False, delete_on_close=False, prefix="Shipit"
        )
        shipit_path = Path(temp_shipit.name)

    if not regenerate:
        if shipit_path and not shipit_path.exists():
            regenerate = True
        elif not (project_paths.workspace_root / "Shipit").exists():
            regenerate = True

    if regenerate:
        generate(
            project_paths.workspace_root,
            subdir=Path(project_paths.subdir) if project_paths.subdir else None,
            out=shipit_path,
            install_command=install_command,
            build_command=build_command,
            start_command=start_command,
            provider=provider,
            config=config,
        )

    build(
        project_paths.workspace_root,
        subdir=Path(project_paths.subdir) if project_paths.subdir else None,
        shipit_path=shipit_path,
        install_command=install_command,
        build_command=build_command,
        start_command=start_command,
        wasmer=wasmer,
        docker=docker,
        docker_client=docker_client,
        docker_opts=docker_opts,
        skip_docker_if_safe_build=skip_docker_if_safe_build,
        wasmer_registry=wasmer_registry,
        wasmer_bin=wasmer_bin,
        skip_prepare=skip_prepare,
        env_name=env_name,
        serve_port=serve_port,
        provider=provider,
        config=config,
    )
    if (
        command_names
        or volume_specs
        or start
        or after_deploy
    ):
        run(
            project_paths.workspace_root,
            wasmer=wasmer,
            wasmer_bin=wasmer_bin,
            docker=docker,
            docker_client=docker_client,
            docker_opts=docker_opts,
            command_names=command_names,
            volume_specs=volume_specs,
            start=start,
            after_deploy=after_deploy,
            wasmer_registry=wasmer_registry,
            serve_port=serve_port,
        )
    
    if wasmer_deploy or wasmer_deploy_config:
        deploy(
            project_paths.workspace_root,
            wasmer_deploy=wasmer_deploy,
            wasmer_deploy_config=wasmer_deploy_config,
            wasmer_bin=wasmer_bin,
            wasmer_token=wasmer_token,
            wasmer_registry=wasmer_registry,
            wasmer_app_owner=wasmer_app_owner,
            wasmer_app_name=wasmer_app_name,
        )


@app.command(name="generate")
def generate(
    path: Path = typer.Argument(
        Path("."),
        help="Project path (defaults to current directory).",
        show_default=False,
    ),
    subdir: Optional[Path] = typer.Option(
        None,
        "--subdir",
        help="App subdirectory relative to the project path.",
    ),
    out: Optional[Path] = typer.Option(
        None,
        "-o",
        "--out",
        "--output",
        "--shipit-path",
        help="Output path (defaults to the Shipit file in the provided path).",
    ),
    install_command: Optional[str] = typer.Option(
        None,
        help="The install command to use (overwrites the default)",
    ),
    build_command: Optional[str] = typer.Option(
        None,
        help="The build command to use (overwrites the default)",
    ),
    start_command: Optional[str] = typer.Option(
        None,
        help="The start command to use (overwrites the default)",
    ),
    provider: Optional[str] = typer.Option(
        None,
        help="Use a specific provider to build the project.",
    ),
    config: Optional[str] = typer.Option(
        None,
        help="The JSON content to use as input.",
    ),
):
    if not path.exists():
        raise Exception(f"The path {path} does not exist")
    project_paths = resolve_project_paths(path, subdir)

    if out is None:
        out = project_paths.workspace_root / "Shipit"

    base_config = Config()
    base_config.commands.enrich_from_path(project_paths.app_path)
    if start_command:
        base_config.commands.start = start_command
    if install_command:
        base_config.commands.install = install_command
    if build_command:
        base_config.commands.build = build_command
    provider_cls = load_provider(
        project_paths.app_path,
        base_config,
        use_provider=provider,
    )
    provider_config = load_provider_config(
        provider_cls,
        project_paths.app_path,
        base_config,
        config=config,
    )
    apply_subdir_provider_config(project_paths, provider_config)
    apply_subdir_workspace_config(project_paths, provider_config)
    provider = provider_cls(project_paths.app_path, provider_config)
    content = generate_shipit(
        project_paths.app_path,
        provider,
        subdir=project_paths.subdir,
    )
    config_json = provider_config.model_dump_json(indent=2, exclude_defaults=True)
    if config_json and config_json != "{}":
        manifest_panel = Panel(
            Syntax(
                config_json,
                "json",
                theme="monokai",
                background_color="default",
                line_numbers=True,
            ),
            box=box.SQUARE,
            border_style="bright_black",
            expand=False,
        )
        console.print(manifest_panel, markup=False, highlight=True)
    out.write_text(content)
    console.print(f"[bold]Generated Shipit[/bold] at {out.absolute()}")


@app.callback(
    invoke_without_command=True,
    context_settings={"allow_extra_args": True, "ignore_unknown_options": True},
)
def _default(ctx: typer.Context) -> None:
    if ctx.invoked_subcommand in ["auto", "generate", "build", "run", "deploy", None]:
        print_help()


@app.command(name="deploy")
def deploy(
    path: Path = typer.Argument(
        Path("."),
        help="Project path (defaults to current directory).",
        show_default=False,
    ),
    wasmer_deploy: bool = typer.Option(
        True,
        help="Deploy the project to Wasmer.",
    ),
    wasmer_bin: Optional[str] = typer.Option(
        None,
        help="The path to the Wasmer binary.",
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
    wasmer_deploy_config: Optional[Path] = typer.Option(
        None,
        help="Save the output of the Wasmer build to a json file",
    ),
) -> None:
    if not path.exists():
        raise Exception(f"The path {path} does not exist")

    build_backend = LocalBuildBackend(path, ASSETS_PATH)
    runner = WasmerRunner(
        build_backend,
        path,
        registry=wasmer_registry,
        token=wasmer_token,
        bin=wasmer_bin,
    )

    if wasmer_deploy_config:
        runner.deploy_config(wasmer_deploy_config)
    elif wasmer_deploy:
        runner.deploy(app_owner=wasmer_app_owner, app_name=wasmer_app_name)


@app.command(name="run")
def run(
    path: Path = typer.Argument(
        Path("."),
        help="Project path (defaults to current directory).",
        show_default=False,
    ),
    wasmer: bool = typer.Option(
        False,
        help="Use Wasmer to run the project.",
    ),
    wasmer_bin: Optional[str] = typer.Option(
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
    docker_opts: Optional[str] = typer.Option(
        None,
        help="Additional options to pass to the Docker client.",
    ),
    command_names: Optional[List[str]] = typer.Option(
        None,
        "-c",
        "--command",
        help="Run one or more commands. Can be passed multiple times.",
    ),
    volume_specs: Optional[List[str]] = typer.Option(
        None,
        "--volume",
        help="Attach one or more volumes as NAME:/guest/path. Can be passed multiple times.",
    ),
    start: bool = typer.Option(
        False,
        "--start/--no-start",
        help="Equivalent to `--command=start`.",
    ),
    after_deploy: bool = typer.Option(
        False,
        "--after-deploy/--no-after-deploy",
        help="Equivalent to `--command=after_deploy`.",
    ),
    wasmer_registry: Optional[str] = typer.Option(
        None,
        help="Wasmer registry.",
    ),
    serve_port: Optional[int] = typer.Option(
        None,
        help="The port to use (defaults to 8080).",
    ),
) -> None:
    if not path.exists():
        raise Exception(f"The path {path} does not exist")

    if docker or docker_client:
        build_backend: BuildBackend = DockerBuildBackend(
            path, ASSETS_PATH, docker_client, docker_opts
        )
    else:
        build_backend = LocalBuildBackend(path, ASSETS_PATH)

    if wasmer:
        runner: Runner = WasmerRunner(
            build_backend,
            path,
            registry=wasmer_registry,
            bin=wasmer_bin,
        )
    else:
        runner = LocalRunner(build_backend, path)

    commands_to_run = resolve_run_commands(
        command_names=command_names,
        start=start,
        after_deploy=after_deploy,
    )

    if commands_to_run:
        run_serve_commands(
            path,
            runner,
            commands_to_run,
            volume_specs=volume_specs,
            env=runtime_serve_env(serve_port),
        )
    else:
        console.print("[bold]No commands specified. Use `--command` to run a command.[/bold]")

@app.command(name="plan")
def plan(
    path: Path = typer.Argument(
        Path("."),
        help="Project path (defaults to current directory).",
        show_default=False,
    ),
    subdir: Optional[Path] = typer.Option(
        None,
        "--subdir",
        help="App subdirectory relative to the project path.",
    ),
    out: Optional[Path] = typer.Option(
        None,
        "-o",
        "--out",
        "--output",
        help="Output path of the plan (defaults to stdout).",
    ),
    temp_shipit: bool = typer.Option(
        False,
        help="Use a temporary Shipit file in the system temporary directory.",
    ),
    regenerate: bool = typer.Option(
        False,
        help="Regenerate the Shipit file.",
    ),
    shipit_path: Optional[Path] = typer.Option(
        None,
        help="The path to the Shipit file (defaults to Shipit in the provided path).",
    ),
    wasmer: bool = typer.Option(
        False,
        help="Use Wasmer to evaluate the project.",
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
        help="Use Docker to evaluate the project.",
    ),
    docker_client: Optional[str] = typer.Option(
        None,
        help="Use a specific Docker client (such as depot, podman, etc.)",
    ),
    install_command: Optional[str] = typer.Option(
        None,
        help="The install command to use (overwrites the default)",
    ),
    build_command: Optional[str] = typer.Option(
        None,
        help="The build command to use (overwrites the default)",
    ),
    start_command: Optional[str] = typer.Option(
        None,
        help="The start command to use (overwrites the default)",
    ),
    provider: Optional[str] = typer.Option(
        None,
        help="Use a specific provider to build the project.",
    ),
    config: Optional[str] = typer.Option(
        None,
        help="The JSON content to use as input.",
    ),
    serve_port: Optional[int] = typer.Option(
        None,
        help="The port to use (defaults to 8080).",
    ),
) -> None:
    if not path.exists():
        raise Exception(f"The path {path} does not exist")
    project_paths = resolve_project_paths(path, subdir)

    if temp_shipit:
        if shipit_path:
            raise Exception("Cannot use both --temp-shipit and --shipit-path")
        temp_shipit = tempfile.NamedTemporaryFile(
            delete=False, delete_on_close=False, prefix="Shipit"
        )
        shipit_path = Path(temp_shipit.name)

    if not regenerate:
        if shipit_path and not shipit_path.exists():
            regenerate = True
        elif not (project_paths.workspace_root / "Shipit").exists():
            regenerate = True

    if regenerate:
        generate(
            project_paths.workspace_root,
            subdir=Path(project_paths.subdir) if project_paths.subdir else None,
            out=shipit_path,
            install_command=install_command,
            build_command=build_command,
            start_command=start_command,
            provider=provider,
            config=config,
        )

    shipit_file = get_shipit_path(project_paths.workspace_root, shipit_path)
    if project_paths.subdir is None:
        project_paths = resolve_project_paths(
            project_paths.workspace_root,
            read_shipit_subdir(shipit_file),
        )

    if docker or docker_client:
        build_backend: BuildBackend = DockerBuildBackend(
            project_paths.workspace_root, ASSETS_PATH, docker_client
        )
    else:
        build_backend = LocalBuildBackend(project_paths.workspace_root, ASSETS_PATH)
    if wasmer:
        runner: Runner = WasmerRunner(
            build_backend,
            project_paths.workspace_root,
            registry=wasmer_registry,
            token=wasmer_token,
            bin=wasmer_bin,
        )
    else:
        runner = LocalRunner(build_backend, project_paths.workspace_root)

    base_config = Config()
    base_config.commands.enrich_from_path(project_paths.app_path)
    if install_command:
        base_config.commands.install = install_command
    if build_command:
        base_config.commands.build = build_command
    if start_command:
        base_config.commands.start = start_command
    if serve_port:
        base_config.port = serve_port
    provider_cls = load_provider(
        project_paths.app_path,
        base_config,
        use_provider=provider,
    )
    provider_config = load_provider_config(
        provider_cls,
        project_paths.app_path,
        base_config,
        config=config,
    )
    apply_subdir_provider_config(project_paths, provider_config)
    apply_subdir_workspace_config(project_paths, provider_config)
    # provider_config = runner.prepare_config(provider_config)
    ctx, serve = evaluate_shipit(shipit_file, build_backend, runner, provider_config)

    def _collect_group_commands(group: str) -> Optional[str]:
        commands = [
            step.command
            for step in serve.build
            if isinstance(step, RunStep) and step.group == group
        ]
        if not commands:
            return None
        return " && ".join(commands)

    start_command = serve.commands.get("start")
    after_deploy_command = serve.commands.get("after_deploy")
    install_command = _collect_group_commands("install")
    build_command = _collect_group_commands("build")
    if start_command:
        provider_config.commands.start = start_command
    if after_deploy_command:
        provider_config.commands.after_deploy = after_deploy_command
    if install_command:
        provider_config.commands.install = install_command
    if build_command:
        provider_config.commands.build = build_command
    plan_output = {
        "provider": provider_cls.name(),
        "config": json.loads(provider_config.model_dump_json(exclude_defaults=True)),
        "services": [
            {"name": svc.name, "provider": svc.provider}
            for svc in (serve.services or [])
        ],
    }
    json_output = json.dumps(plan_output, indent=4)
    if out:
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(json_output)
        console.print(f"[bold]Plan saved to {out.absolute()}[/bold]")
    else:
        sys.stdout.write(json_output + "\n")
        sys.stdout.flush()


@app.command(name="build")
def build(
    path: Path = typer.Argument(
        Path("."),
        help="Project path (defaults to current directory).",
        show_default=False,
    ),
    subdir: Optional[Path] = typer.Option(
        None,
        "--subdir",
        help="App subdirectory relative to the project path.",
    ),
    shipit_path: Optional[Path] = typer.Option(
        None,
        help="The path to the Shipit file (defaults to Shipit in the provided path).",
    ),
    start_command: Optional[str] = typer.Option(
        None,
        help="The start command to use (overwrites the default)",
    ),
    install_command: Optional[str] = typer.Option(
        None,
        help="The install command to use (overwrites the default)",
    ),
    build_command: Optional[str] = typer.Option(
        None,
        help="The build command to use (overwrites the default)",
    ),
    wasmer: bool = typer.Option(
        False,
        help="Use Wasmer to build and package the project.",
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
    docker_opts: Optional[str] = typer.Option(
        None,
        help="Additional options to pass to the Docker client.",
    ),
    skip_docker_if_safe_build: Optional[bool] = typer.Option(
        True,
        help="Skip Docker if the build can be done safely locally (only copy commands).",
    ),
    env_name: Optional[str] = typer.Option(
        None,
        help="The environment to use (defaults to `.env`, it will use .env.<env_name> if provided)",
    ),
    serve_port: Optional[int] = typer.Option(
        None,
        help="The port to use (defaults to 8080).",
    ),
    provider: Optional[str] = typer.Option(
        None,
        help="Use a specific provider to build the project.",
    ),
    config: Optional[str] = typer.Option(
        None,
        help="The JSON content to use as input.",
    ),
) -> None:
    if not path.exists():
        raise Exception(f"The path {path} does not exist")

    shipit_file = get_shipit_path(path, shipit_path)
    project_paths = resolve_project_paths(
        path,
        subdir or read_shipit_subdir(shipit_file),
    )

    if docker or docker_client:
        build_backend: BuildBackend = DockerBuildBackend(
            project_paths.workspace_root,
            ASSETS_PATH,
            docker_client,
            docker_opts=docker_opts,
        )
    else:
        build_backend = LocalBuildBackend(project_paths.workspace_root, ASSETS_PATH)
    if wasmer:
        runner: Runner = WasmerRunner(
            build_backend,
            project_paths.workspace_root,
            registry=wasmer_registry,
            token=wasmer_token,
            bin=wasmer_bin,
        )
    else:
        runner = LocalRunner(build_backend, project_paths.workspace_root)

    base_config = Config()
    base_config.commands.enrich_from_path(project_paths.app_path)
    if start_command:
        base_config.commands.start = start_command
    if install_command:
        base_config.commands.install = install_command
    if build_command:
        base_config.commands.build = build_command
    serve_port = serve_port or os.environ.get("PORT")
    if serve_port:
        base_config.port = serve_port

    provider_cls = load_provider(
        project_paths.app_path,
        base_config,
        use_provider=provider,
    )
    provider_config = load_provider_config(
        provider_cls,
        project_paths.app_path,
        base_config,
        config=config,
    )
    apply_subdir_provider_config(project_paths, provider_config)
    apply_subdir_workspace_config(project_paths, provider_config)
    provider_config = runner.prepare_config(provider_config)
    ctx, serve = evaluate_shipit(shipit_file, build_backend, runner, provider_config)
    env = {
        "PATH": "",
        "COLORTERM": os.environ.get("COLORTERM", ""),
        "LSCOLORS": os.environ.get("LSCOLORS", "0"),
        "LS_COLORS": os.environ.get("LS_COLORS", "0"),
        "CLICOLOR": os.environ.get("CLICOLOR", "0"),
    }

    if skip_docker_if_safe_build and serve.build and len(serve.build) > 0:
        # If it doesn't have a run step, then it's safe to skip Docker and run all the
        # steps locally.
        has_run = any(isinstance(step, RunStep) for step in serve.build)
        if not has_run:
            console.print(
                f"[bold]ℹ️ Building locally instead of Docker to speed up the build, as all commands are safe to run locally[/bold]"
            )
            return build(
                project_paths.workspace_root,
                subdir=Path(project_paths.subdir)
                if project_paths.subdir
                else None,
                shipit_path=shipit_path,
                install_command=install_command,
                build_command=build_command,
                start_command=start_command,
                wasmer=wasmer,
                skip_prepare=skip_prepare,
                wasmer_bin=wasmer_bin,
                wasmer_registry=wasmer_registry,
                wasmer_token=wasmer_token,
                docker=False,
                docker_client=None,
                docker_opts=None,
                skip_docker_if_safe_build=False,
                env_name=env_name,
                serve_port=serve_port,
                provider=provider,
                config=config,
            )

    serve.env = serve.env or {}
    load_env_files(project_paths, env_name, serve.env)

    assert serve.commands.get("start"), (
        "No start command could be found, please provide a start command"
    )

    # Prepare the build steps (sometimes the runners need to adapt the dependencies)
    build_steps = runner.prepare_build_steps(serve.build)

    # Build and serve
    build_backend.build(serve.name, env, serve.mounts or [], build_steps)
    build_volumes(project_paths.workspace_root, serve)
    runner.build(serve)
    if serve.prepare and not skip_prepare:
        console.print("\n[bold]Running prepare step[/bold]")
        runner.prepare(env, serve.prepare)


def get_shipit_path(path: Path, shipit_path: Optional[Path] = None) -> Path:
    if shipit_path is None:
        shipit_path = path / "Shipit"
        if not shipit_path.exists():
            raise Exception(
                f"Shipit file not found at {shipit_path}. Run `shipit generate {path}` to create it."
            )
    elif not shipit_path.exists():
        raise Exception(
            f"Shipit file not found at {shipit_path}. Run `shipit generate {path} -o {shipit_path}` to create it."
        )
    return shipit_path


def resolve_run_commands(
    command_names: Optional[List[str]],
    start: bool,
    after_deploy: bool,
) -> List[str]:
    commands = list(command_names or [])
    if after_deploy and "after_deploy" not in commands:
        commands.append("after_deploy")
    if start and "start" not in commands:
        commands.append("start")
    return commands


def run_serve_commands(
    path: Path,
    runner: Runner,
    commands: List[str],
    volume_specs: Optional[List[str]] = None,
    env: Optional[Dict[str, str]] = None,
) -> None:
    volume_mappings = merge_volume_mappings(
        load_volume_mappings(path),
        parse_cli_volume_mappings(volume_specs),
    )
    for command in commands:
        if command in OPTIONAL_RUN_COMMANDS and not runner.has_serve_command(command):
            continue
        console.print(f"\nRunning command [bold]{command}[/bold]")
        runner.run_serve_command(
            command,
            volume_mappings=volume_mappings,
            env=env,
        )


def runtime_serve_env(serve_port: Optional[int]) -> Dict[str, str]:
    if serve_port is not None:
        port = str(serve_port)
    else:
        port = os.environ.get("PORT", "8080")
    return {"PORT": port}

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
        if os.environ.get("SHIPIT_DEBUG", "false").lower() in ["1", "true", "yes", "y"]:
            raise e


if __name__ == "__main__":
    main()


def flatten(xss):
    return [x for xs in xss for x in xs]
