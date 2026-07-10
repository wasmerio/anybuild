"""Shipit plan evaluation.

Evaluates a Shipit file (and its load() graph of stdlib/provider modules)
into a Serve plan. The builtins exposed to Starlark are methods of a per-run
Ctx; they return plan objects directly and serve() receives them back.
"""

from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict, List, Literal, Optional, Tuple

import xingque as sl

from shipit.builders.base import BuildBackend
from shipit.providers.base import Config
from shipit.runners.base import Runner
from shipit.shipit_types import (
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
from shipit.starlark_loader import config_view, eval_module_graph, globals_builder


@dataclass
class CtxMount:
    """Starlark-facing handle for a mount: string paths plus the plan Mount."""

    mount: Mount
    path: str
    serve_path: str



class Ctx:
    def __init__(
        self,
        build_backend: BuildBackend,
        runner: Runner,
        source_dir: Optional[Path] = None,
    ) -> None:
        self.build_backend = build_backend
        self.runner = runner
        self.source_dir = source_dir
        self.serves: Dict[str, Serve] = {}

    def file_exists(self, path: str) -> bool:
        """Read-only probe of the app source tree, exposed to Shipit files.

        Paths are relative to the app directory (the subdir for subdir
        projects) and may not escape it. Matches files and directories.
        """
        if self.source_dir is None:
            raise ValueError("file_exists() is not available in this context")
        candidate = Path(path)
        if candidate.is_absolute():
            raise ValueError(
                f"file_exists() requires a project-relative path, got {path!r}"
            )
        # The escape check is lexical: in-project symlinks may legitimately
        # point outside the tree, so the target is never resolved.
        if ".." in candidate.parts:
            raise ValueError(
                f"file_exists() path escapes the project: {path!r}"
            )
        return (self.source_dir / candidate).exists()

    # The builtins below return plan objects directly; Starlark passes them
    # around opaquely and serve() receives them back unchanged.

    def dep(
        self,
        name: str,
        version: Optional[str] = None,
        architecture: Optional[Literal["64-bit", "32-bit"]] = None,
    ) -> Package:
        return Package(name, version, architecture)

    def service(
        self, name: str, provider: Literal["postgres", "mysql", "redis"]
    ) -> Service:
        return Service(name, provider)

    def serve(
        self,
        name: str,
        provider: str,
        build: List[Optional[Step]],
        deps: List[Package],
        commands: Dict[str, str],
        cwd: Optional[str] = None,
        prepare: Optional[List[Optional[Step]]] = None,
        mounts: Optional[List["CtxMount"]] = None,
        volumes: Optional[List[dict]] = None,
        env: Optional[Dict[str, str]] = None,
        services: Optional[List[Service]] = None,
    ) -> Serve:
        prepare_steps: Optional[List[PrepareStep]] = None
        if prepare is not None:
            # Conditional steps evaluate to None; prepare only runs commands.
            prepare_steps = [s for s in prepare if isinstance(s, RunStep)]
        serve = Serve(
            name=name,
            provider=provider,
            build=[step for step in build if step is not None],
            cwd=cwd,
            deps=[dep for dep in deps if dep is not None],
            commands=commands,
            prepare=prepare_steps,
            mounts=[m.mount for m in mounts if m is not None] if mounts else None,
            volumes=[v["volume"] for v in volumes if v is not None] if volumes else None,
            env=env,
            services=[s for s in services if s is not None] if services else None,
        )
        self.serves[name] = serve
        return serve

    def path(self, path: str) -> PathStep:
        return PathStep(path)

    def use(self, *dependencies: Package) -> UseStep:
        return UseStep(list(dependencies))

    def run(self, *args: Any, **kwargs: Any) -> RunStep:
        return RunStep(*args, **kwargs)

    def workdir(self, path: str) -> WorkdirStep:
        return WorkdirStep(Path(path))

    def copy(
        self,
        source: str,
        target: Optional[str] = None,
        ignore: Optional[List[str]] = None,
        base: Optional[Literal["source", "assets"]] = None,
    ) -> CopyStep:
        if target is None:
            target = source
        return CopyStep(source, target, ignore, base or "source")

    def env(self, **env_vars: str) -> EnvStep:
        return EnvStep(env_vars)

    def write(self, path: str, content: str) -> WriteFileStep:
        return WriteFileStep(path, content)

    def mount(self, name: str) -> "CtxMount":
        build_path = self.build_backend.get_build_mount_path(name)
        serve_path = self.runner.get_serve_mount_path(name)
        mount = Mount(name, build_path, serve_path)
        return CtxMount(
            mount=mount,
            path=str(build_path.absolute()),
            serve_path=str(serve_path.absolute()),
        )

    def volume(self, name: str, serve: str) -> dict:
        volume = Volume(
            name=name,
            path=self.build_backend.get_volume_path(name),
            serve_path=Path(serve),
        )
        return {
            "volume": volume,
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
    project_root: Optional[Path] = None,
) -> Tuple[Ctx, Serve]:
    source = shipit_file.read_text()
    workspace_root = project_root or shipit_file.resolve().parent
    # file_exists() probes the app directory: the subdir for subdir projects.
    source_dir = workspace_root
    app_subdir = getattr(provider_config, "app_subdir", None)
    if app_subdir:
        source_dir = workspace_root / app_subdir
    ctx = Ctx(build_backend, runner, source_dir=source_dir)

    def set_builtins(glb: "sl.GlobalsBuilder") -> None:
        glb.set("PORT", str(provider_config.port or "8080"))
        glb.set("service", ctx.service)
        glb.set("dep", ctx.dep)
        glb.set("serve", ctx.serve)
        # The stdlib's serve() assembler (//shipit:serve.shipit) shadows the
        # builtin by design; it reaches the raw builtin through this alias.
        glb.set("_serve", ctx.serve)
        glb.set("run", ctx.run)
        glb.set("mount", ctx.mount)
        glb.set("volume", ctx.volume)
        glb.set("workdir", ctx.workdir)
        glb.set("copy", ctx.copy)
        glb.set("write", ctx.write)
        glb.set("path", ctx.path)
        glb.set("env", ctx.env)
        glb.set("use", ctx.use)
        glb.set("file_exists", ctx.file_exists)

    # Library modules get the builtins but not `config`: provider libraries
    # receive the config as a function parameter, keeping them pure.
    lib_glb = globals_builder()
    set_builtins(lib_glb)

    entry_glb = globals_builder()
    set_builtins(entry_glb)
    entry_glb.set("config", config_view(provider_config))

    eval_module_graph(
        source=source,
        entry_path=shipit_file,
        entry_globals=entry_glb.build(),
        lib_globals=lib_glb.build(),
        project_root=workspace_root,
    )
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
        new_build: List[Step] = []
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

