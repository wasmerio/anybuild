import tempfile
import os
import sys
import json
import re
from pathlib import Path
from dataclasses import dataclass
from typing import Any, Dict, List, Optional

import typer
from rich import box
from rich.panel import Panel
from rich.syntax import Syntax

from shipit.generator import generate_shipit, load_provider, load_provider_config
from shipit.providers.base import Config
# Re-exported for tests and downstream users of the old import location.
from shipit.evaluator import Ctx, CtxMount, evaluate_shipit  # noqa: F401
from dotenv import dotenv_values
from shipit.builders import BuildBackend, DockerBuildBackend, LocalBuildBackend
from shipit.runners import Runner, LocalRunner, WasmerRunner
from shipit.shipit_types import RunStep
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


@dataclass(frozen=True)
class ProjectPaths:
    workspace_root: Path
    app_path: Path
    subdir: Optional[str] = None


def shipit_subdir_slug(subdir: str) -> str:
    slug = re.sub(r"[^A-Za-z0-9._-]+", "-", subdir.replace("/", "-"))
    return slug.strip("-") or "app"


def default_shipit_path(project_paths: ProjectPaths) -> Path:
    if project_paths.subdir is None:
        return project_paths.workspace_root / "Shipit"
    return (
        project_paths.workspace_root
        / f"Shipit.{shipit_subdir_slug(project_paths.subdir)}"
    )


def default_shipit_dir(project_paths: ProjectPaths) -> Path:
    shipit_dir = project_paths.workspace_root / ".shipit"
    if project_paths.subdir is None:
        return shipit_dir
    return shipit_dir / shipit_subdir_slug(project_paths.subdir)


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
                target.update(
                    {
                        key: value
                        for key, value in dotenv_values(env_file).items()
                        if value is not None
                    }
                )


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



@dataclass
class ProjectContext:
    """Everything the CLI commands need after resolving and evaluating."""

    paths: ProjectPaths
    shipit_file: Path
    shipit_dir: Path
    build_backend: BuildBackend
    runner: Runner
    provider_cls: Any
    provider_config: Config
    ctx: Ctx
    serve: Any


def resolve_environment(
    project_paths: ProjectPaths,
    *,
    wasmer: bool = False,
    wasmer_bin: Optional[str] = None,
    wasmer_registry: Optional[str] = None,
    wasmer_token: Optional[str] = None,
    docker: bool = False,
    docker_client: Optional[str] = None,
    docker_opts: Optional[str] = None,
):
    """Build backend + runner for an already-resolved project."""
    shipit_dir = default_shipit_dir(project_paths)
    if docker or docker_client:
        build_backend: BuildBackend = DockerBuildBackend(
            project_paths.workspace_root,
            ASSETS_PATH,
            docker_client,
            docker_opts=docker_opts,
            shipit_dir=shipit_dir,
        )
    else:
        build_backend = LocalBuildBackend(
            project_paths.workspace_root,
            ASSETS_PATH,
            shipit_dir=shipit_dir,
        )
    if wasmer:
        runner: Runner = WasmerRunner(
            build_backend,
            project_paths.workspace_root,
            registry=wasmer_registry,
            token=wasmer_token,
            bin=wasmer_bin,
            shipit_dir=shipit_dir,
        )
    else:
        runner = LocalRunner(
            build_backend,
            project_paths.workspace_root,
            shipit_dir=shipit_dir,
        )
    return shipit_dir, build_backend, runner


def resolve_project_context(
    path: Path,
    subdir: Optional[Path] = None,
    *,
    shipit_path: Optional[Path] = None,
    wasmer: bool = False,
    wasmer_bin: Optional[str] = None,
    wasmer_registry: Optional[str] = None,
    wasmer_token: Optional[str] = None,
    docker: bool = False,
    docker_client: Optional[str] = None,
    docker_opts: Optional[str] = None,
    start_command: Optional[str] = None,
    install_command: Optional[str] = None,
    build_command: Optional[str] = None,
    serve_port: Optional[int] = None,
    use_provider: Optional[str] = None,
    config: Optional[str] = None,
) -> ProjectContext:
    """The shared pipeline behind the CLI commands.

    Resolves paths (including the subdir recorded in the Shipit file), builds
    the backend/runner pair, loads the provider config, applies the runner's
    config hook, and evaluates the Shipit file into a plan. Every command
    that needs a plan goes through here so the flows cannot drift.
    """
    if not path.exists():
        raise Exception(f"The path {path} does not exist")
    project_paths = resolve_project_paths(path, subdir)
    shipit_file = get_shipit_path(project_paths, shipit_path)
    if project_paths.subdir is None:
        project_paths = resolve_project_paths(
            project_paths.workspace_root,
            read_shipit_subdir(shipit_file),
        )
    shipit_dir, build_backend, runner = resolve_environment(
        project_paths,
        wasmer=wasmer,
        wasmer_bin=wasmer_bin,
        wasmer_registry=wasmer_registry,
        wasmer_token=wasmer_token,
        docker=docker,
        docker_client=docker_client,
        docker_opts=docker_opts,
    )

    base_config = Config()
    base_config.commands.enrich_from_path(project_paths.app_path)
    if start_command:
        base_config.commands.start = start_command
    if install_command:
        base_config.commands.install = install_command
    if build_command:
        base_config.commands.build = build_command
    if serve_port is None:
        env_port = os.environ.get("PORT")
        if env_port and env_port.isdigit():
            serve_port = int(env_port)
    if serve_port:
        base_config.port = serve_port

    provider_cls = load_provider(
        project_paths.app_path,
        base_config,
        use_provider=use_provider,
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
    ctx, serve = evaluate_shipit(
        shipit_file,
        build_backend,
        runner,
        provider_config,
        project_root=project_paths.workspace_root,
    )
    return ProjectContext(
        paths=project_paths,
        shipit_file=shipit_file,
        shipit_dir=shipit_dir,
        build_backend=build_backend,
        runner=runner,
        provider_cls=provider_cls,
        provider_config=provider_config,
        ctx=ctx,
        serve=serve,
    )


def print_help() -> None:
    panel = Panel(
        f"Shipit {shipit_version}",
        box=box.ROUNDED,
        border_style="blue",
        expand=False,
    )
    console.print(panel)


def version_callback(value: bool) -> bool:
    if value:
        typer.echo(shipit_version)
        raise typer.Exit()
    return value


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
        help="The path to the Shipit file (defaults to Shipit or Shipit.<subdir>).",
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
        temp_shipit_file = tempfile.NamedTemporaryFile(
            delete=False, delete_on_close=False, prefix="Shipit"
        )
        shipit_path = Path(temp_shipit_file.name)

    if not regenerate:
        if shipit_path and not shipit_path.exists():
            regenerate = True
        elif not shipit_path and not default_shipit_path(project_paths).exists():
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
            subdir=Path(project_paths.subdir) if project_paths.subdir else None,
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
            subdir=Path(project_paths.subdir) if project_paths.subdir else None,
            wasmer_deploy=bool(wasmer_deploy),
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
        help="Output path (defaults to Shipit or Shipit.<subdir>).",
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
        out = default_shipit_path(project_paths)

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
    provider_instance = provider_cls(project_paths.app_path, provider_config)
    content = generate_shipit(
        project_paths.app_path,
        provider_instance,
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
def _default(
    ctx: typer.Context,
    version: bool = typer.Option(
        False,
        "--version",
        "-v",
        callback=version_callback,
        is_eager=True,
        help="Show the version and exit.",
    ),
) -> None:
    if ctx.invoked_subcommand in ["auto", "generate", "build", "run", "deploy", None]:
        print_help()


@app.command(name="deploy")
def deploy(
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
    project_paths = resolve_project_paths(path, subdir)
    _shipit_dir, _build_backend, runner = resolve_environment(
        project_paths,
        wasmer=True,
        wasmer_bin=wasmer_bin,
        wasmer_registry=wasmer_registry,
        wasmer_token=wasmer_token,
    )
    assert isinstance(runner, WasmerRunner)

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
    subdir: Optional[Path] = typer.Option(
        None,
        "--subdir",
        help="App subdirectory relative to the project path.",
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
    project_paths = resolve_project_paths(path, subdir)
    shipit_dir, _build_backend, runner = resolve_environment(
        project_paths,
        wasmer=wasmer,
        wasmer_bin=wasmer_bin,
        wasmer_registry=wasmer_registry,
        docker=docker,
        docker_client=docker_client,
        docker_opts=docker_opts,
    )

    commands_to_run = resolve_run_commands(
        command_names=command_names,
        start=start,
        after_deploy=after_deploy,
    )

    if commands_to_run:
        run_serve_commands(
            project_paths.workspace_root,
            runner,
            commands_to_run,
            volume_specs=volume_specs,
            env=runtime_serve_env(serve_port),
            shipit_dir=shipit_dir,
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
        help="The path to the Shipit file (defaults to Shipit or Shipit.<subdir>).",
    ),
    wasmer: bool = typer.Option(
        False,
        help="Use Wasmer to evaluate the project.",
    ),
    wasmer_bin: Optional[str] = typer.Option(
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
        temp_shipit_file = tempfile.NamedTemporaryFile(
            delete=False, delete_on_close=False, prefix="Shipit"
        )
        shipit_path = Path(temp_shipit_file.name)

    if not regenerate:
        if shipit_path and not shipit_path.exists():
            regenerate = True
        elif not shipit_path and not default_shipit_path(project_paths).exists():
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

    context = resolve_project_context(
        project_paths.workspace_root,
        Path(project_paths.subdir) if project_paths.subdir else None,
        shipit_path=shipit_path,
        wasmer=wasmer,
        wasmer_bin=wasmer_bin,
        wasmer_registry=wasmer_registry,
        wasmer_token=wasmer_token,
        docker=docker,
        docker_client=docker_client,
        start_command=start_command,
        install_command=install_command,
        build_command=build_command,
        serve_port=serve_port,
        use_provider=provider,
        config=config,
    )
    serve = context.serve
    provider_cls = context.provider_cls
    provider_config = context.provider_config

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
        help="The path to the Shipit file (defaults to Shipit or Shipit.<subdir>).",
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
    wasmer_bin: Optional[str] = typer.Option(
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
    context = resolve_project_context(
        path,
        subdir,
        shipit_path=shipit_path,
        wasmer=wasmer,
        wasmer_bin=wasmer_bin,
        wasmer_registry=wasmer_registry,
        wasmer_token=wasmer_token,
        docker=docker,
        docker_client=docker_client,
        docker_opts=docker_opts,
        start_command=start_command,
        install_command=install_command,
        build_command=build_command,
        serve_port=serve_port,
        use_provider=provider,
        config=config,
    )
    project_paths = context.paths
    shipit_dir = context.shipit_dir
    build_backend = context.build_backend
    runner = context.runner
    serve = context.serve
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
    build_volumes(project_paths.workspace_root, serve, shipit_dir=shipit_dir)
    runner.build(serve)
    if serve.prepare and not skip_prepare:
        console.print("\n[bold]Running prepare step[/bold]")
        runner.prepare(env, serve.prepare)


def get_shipit_path(
    project_paths: ProjectPaths,
    shipit_path: Optional[Path] = None,
) -> Path:
    if shipit_path is None:
        shipit_path = default_shipit_path(project_paths)
        if not shipit_path.exists():
            command = f"shipit generate {project_paths.workspace_root}"
            if project_paths.subdir:
                command = f"{command} --subdir={project_paths.subdir}"
            raise Exception(
                f"Shipit file not found at {shipit_path}. Run `{command}` to create it."
            )
    elif not shipit_path.exists():
        raise Exception(
            f"Shipit file not found at {shipit_path}. Run `shipit generate {project_paths.workspace_root} -o {shipit_path}` to create it."
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
    shipit_dir: Optional[Path] = None,
) -> None:
    volume_mappings = merge_volume_mappings(
        load_volume_mappings(path, shipit_dir=shipit_dir),
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
    if not args or (
        args[0] not in {"--version", "-v"}
        and (args[0].startswith("-") or args[0] not in available_commands)
    ):
        sys.argv = [sys.argv[0], "auto", *args]

    try:
        app()
    except Exception as e:
        console.print(f"[bold red]{type(e).__name__}[/bold red]: {e}")
        if os.environ.get("SHIPIT_DEBUG", "false").lower() in ["1", "true", "yes", "y"]:
            raise e
        sys.exit(1)


if __name__ == "__main__":
    main()


def flatten(xss):
    return [x for xs in xss for x in xs]
