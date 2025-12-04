import shutil
from pathlib import Path
from typing import Dict, List, Optional

import sh  # type: ignore[import-untyped]
from rich import box
from rich.panel import Panel
from rich.syntax import Syntax

from shipit.builders.base import BuildBackend
from shipit.runners.base import Runner
from shipit.shipit_types import PrepareStep, Serve
from shipit.ui import console, write_stderr, write_stdout


class LocalRunner:
    def __init__(self, build_backend: BuildBackend, src_dir: Path) -> None:
        self.build_backend = build_backend
        self.src_dir = src_dir
        self.runner_path = self.src_dir / ".shipit" / "runner" / "local"
        self.serve_bin_path = self.runner_path / "serve" / "bin"
        self.prepare_bash_script = self.runner_path / "prepare" / "prepare.sh"

    def prepare_config(self, provider_config: object) -> object:
        return provider_config

    def get_serve_mount_path(self, name: str) -> Path:
        return self.build_backend.get_artifact_mount_path(name)

    def build(self, serve: Serve) -> None:
        self.build_prepare(serve)
        self.build_serve(serve)

    def build_prepare(self, serve: Serve) -> None:
        if not serve.prepare:
            return
        self.prepare_bash_script.parent.mkdir(parents=True, exist_ok=True)
        commands: List[str] = []
        if serve.cwd:
            commands.append(f"cd {serve.cwd}")
        if serve.prepare:
            for step in serve.prepare:
                commands.append(step.command)
        content = "#!/bin/bash\n{body}".format(body="\n".join(commands))
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
        self.prepare_bash_script.write_text(content)
        self.prepare_bash_script.chmod(0o755)

    def build_serve(self, serve: Serve) -> None:
        console.print("\n[bold]Building serve[/bold]")
        shutil.rmtree(self.serve_bin_path.parent, ignore_errors=True)
        self.serve_bin_path.mkdir(parents=True, exist_ok=True)
        runtime_path = self.build_backend.get_runtime_path() or ""
        path_prefix = f"PATH={runtime_path}:$PATH " if runtime_path else ""
        console.print(f"[bold]Serve Commands:[/bold]")
        for command in serve.commands:
            console.print(f"* {command}")
            command_path = self.serve_bin_path / command
            env_vars = ""
            if serve.env:
                env_vars = " ".join([f"{k}={v}" for k, v in serve.env.items()])
            lines = ["#!/bin/bash"]
            if serve.cwd:
                lines.append(f"cd {serve.cwd}")
            cmd_body = f"{path_prefix}{env_vars} {serve.commands[command]}".strip()
            lines.append(cmd_body)
            content = "\n".join(lines) + "\n"
            command_path.write_text(content)
            manifest_panel = Panel(
                Syntax(
                    content.strip(),
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
            command_path.chmod(0o755)

    def prepare(self, env: Dict[str, str], prepare: List[PrepareStep]) -> None:
        sh.Command(str(self.prepare_bash_script))(_out=write_stdout, _err=write_stderr)

    def run_serve_command(self, command: str) -> None:
        console.print(f"\n[bold]Running {command} command[/bold]")
        command_path = self.serve_bin_path / command
        sh.Command(str(command_path))(_out=write_stdout, _err=write_stderr)
