import os
import shlex
import shutil
from pathlib import Path
from typing import Dict, List, Optional

import sh  # type: ignore[import-untyped]
from rich.rule import Rule
from rich.syntax import Syntax

from shipit.builders.base import BuildBackend
from shipit.shipit_types import (
    CopyStep,
    EnvStep,
    Mount,
    PathStep,
    RunStep,
    Step,
    UseStep,
    WorkdirStep,
)
from shipit.ui import console, write_stderr, write_stdout
from shipit.utils import download_file


class LocalBuildBackend:
    def __init__(self, src_dir: Path, assets_path: Path) -> None:
        self.src_dir = src_dir
        self.assets_path = assets_path
        self.local_path = self.src_dir / ".shipit" / "local"
        self.build_path = self.local_path / "build"
        self.workdir = self.build_path
        self.runtime_path: Optional[str] = None

    def get_mount_path(self, name: str) -> Path:
        if name == "app":
            return self.build_path / "app"
        else:
            return self.build_path / "opt" / name

    def get_build_mount_path(self, name: str) -> Path:
        return self.get_mount_path(name)

    def get_artifact_mount_path(self, name: str) -> Path:
        return self.get_mount_path(name)

    def execute_step(self, step: Step, env: Dict[str, str]) -> None:
        build_path = self.workdir
        if isinstance(step, UseStep):
            console.print(
                f"[bold]Using dependencies:[/bold] {', '.join([str(dep) for dep in step.dependencies])}"
            )
        elif isinstance(step, WorkdirStep):
            console.print(f"[bold]Working in {step.path}[/bold]")
            self.workdir = step.path
            step.path.mkdir(parents=True, exist_ok=True)
        elif isinstance(step, RunStep):
            extra = ""
            if step.inputs:
                for input in step.inputs:
                    console.print(f"Copying {input} to {build_path / input}")
                    shutil.copy((self.src_dir / input), (build_path / input))
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
            PATH = os.pathsep.join(extended_paths)
            exe = shutil.which(program, path=PATH)
            if not exe:
                raise Exception(f"Program is not installed: {program}")
            cmd = sh.Command("bash")
            cmd(
                "-c",
                command_line,
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
            ignore_matches = list(step.ignore or [])
            ignore_matches.append(".shipit")
            ignore_matches.append("Shipit")

            if step.is_download():
                console.print(
                    f"[bold]Download from {step.source} to {step.target}[/bold]"
                )
                download_file(step.source, (build_path / step.target))
            else:
                if step.base == "source":
                    base = self.src_dir
                elif step.base == "assets":
                    base = self.assets_path
                else:
                    raise Exception(f"Unknown base: {step.base}")

                console.print(
                    f"[bold]Copy to {step.target} from {step.source}[/bold]{ignore_extra}"
                )

                source = base / step.source
                target = build_path / step.target
                if source.is_dir():
                    shutil.copytree(
                        source,
                        target,
                        dirs_exist_ok=True,
                        ignore=shutil.ignore_patterns(*ignore_matches),
                    )
                elif source.is_file():
                    shutil.copy(source, target)
                else:
                    raise Exception(f"Source {step.source} is not a file or directory")
        elif isinstance(step, EnvStep):
            console.print(f"Setting environment variables: {step}")
            env.update(step.variables)
        elif isinstance(step, PathStep):
            console.print(f"[bold]Add {step.path}[/bold] to PATH")
            fullpath = step.path
            env["PATH"] = f"{fullpath}{os.pathsep}{env['PATH']}"
        else:
            raise Exception(f"Unknown step type: {type(step)}")

    def build(
        self, name: str, env: Dict[str, str], mounts: List[Mount], steps: List[Step]
    ) -> None:
        console.print(f"\n[bold]Building... 🚀[/bold]")
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
            path.write_text(env["PATH"])
        self.runtime_path = env.get("PATH")

        console.print(Rule(characters="-", style="bright_black"))
        console.print(f"[bold]Build complete ✅[/bold]")

    def get_runtime_path(self) -> Optional[str]:
        return self.runtime_path
