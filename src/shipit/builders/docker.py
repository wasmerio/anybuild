import base64
import json
import os
import shutil
from pathlib import Path
from typing import Dict, List, Optional

import sh  # type: ignore[import-untyped]
from rich import box
from rich.panel import Panel
from rich.rule import Rule
from rich.syntax import Syntax

from shipit.builders.base import BuildBackend
from shipit.shipit_types import (
    CopyStep,
    EnvStep,
    Mount,
    Package,
    PathStep,
    RunStep,
    Step,
    UseStep,
    WorkdirStep,
)
from shipit.ui import console, write_stderr, write_stdout


class DockerBuildBackend:
    mise_mapper = {
        "php": {
            "source": "ubi:adwinying/php",
        },
        "composer": {
            "source": "ubi:composer/composer",
            "postinstall": """composer_dir=$(mise where ubi:composer/composer); ln -s "$composer_dir/composer.phar" /usr/local/bin/composer""",
        },
    }

    def __init__(
        self, src_dir: Path, assets_path: Path, docker_client: Optional[str] = None
    ) -> None:
        self.src_dir = src_dir
        self.assets_path = assets_path
        self.docker_path = self.src_dir / ".shipit" / "docker"
        self.docker_path.mkdir(parents=True, exist_ok=True)
        self.docker_out_path = self.docker_path / "out"
        self.docker_file_path = self.docker_path / "Dockerfile"
        self.docker_name_path = self.docker_path / "name"
        self.docker_ignore_path = self.docker_path / "Dockerfile.dockerignore"
        self.shipit_docker_path = Path("/shipit")
        self.docker_client = docker_client or "docker"
        self.env = {
            "HOME": "/root",
        }
        self.runtime_path: Optional[str] = None

    def get_mount_path(self, name: str) -> Path:
        if name == "app":
            return Path("app")
        else:
            return Path("opt") / name

    def get_build_mount_path(self, name: str) -> Path:
        return Path("/") / self.get_mount_path(name)

    def get_artifact_mount_path(self, name: str) -> Path:
        return self.docker_out_path / self.get_mount_path(name)

    @property
    def is_depot(self) -> bool:
        return self.docker_client == "depot"

    def build_dockerfile(self, image_name: str, contents: str) -> None:
        self.docker_file_path.write_text(contents)
        self.docker_name_path.write_text(image_name)
        self.print_dockerfile(contents)
        extra_args: List[str] = []
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
            _env=os.environ,
            _out=write_stdout,
            _err=write_stderr,
        )

    def print_dockerfile(self, contents: str) -> None:
        manifest_panel = Panel(
            Syntax(
                contents,
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

    def build(
        self, name: str, env: Dict[str, str], mounts: List[Mount], steps: List[Step]
    ) -> None:
        docker_file_contents = ""

        base_path = self.docker_path
        shutil.rmtree(base_path, ignore_errors=True)
        base_path.mkdir(parents=True, exist_ok=True)

        docker_file_contents = "# syntax=docker/dockerfile:1.7-labs\n"
        docker_file_contents += "FROM debian:trixie-slim AS build\n"
        docker_file_contents += """
RUN apt-get update \\
    && apt-get -y --no-install-recommends install \\
        build-essential gcc make autoconf libtool bison \\
        dpkg-dev pkg-config re2c locate \\
        libmariadb-dev libmariadb-dev-compat libpq-dev \\
        libvips-dev default-libmysqlclient-dev libmagickwand-dev \\
        libicu-dev libxml2-dev libxslt-dev libyaml-dev \\
        sudo curl ca-certificates \\
    && rm -rf /var/lib/apt/lists/*

SHELL ["/bin/bash", "-o", "pipefail", "-c"]
ENV MISE_DATA_DIR="/mise"
ENV MISE_CONFIG_DIR="/mise"
ENV MISE_CACHE_DIR="/mise/cache"
ENV MISE_INSTALL_PATH="/usr/local/bin/mise"
ENV PATH="/mise/shims:$PATH"

RUN curl https://mise.run | sh
"""
        for mount in mounts:
            docker_file_contents += f"RUN mkdir -p {mount.build_path.absolute()}\n"

        for step in steps:
            if isinstance(step, WorkdirStep):
                docker_file_contents += f"WORKDIR {step.path.absolute()}\n"
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
                docker_file_contents += f"RUN {pre}{step.command}\n"
            elif isinstance(step, CopyStep):
                if step.is_download():
                    docker_file_contents += (
                        "ADD " + step.source + " " + step.target + "\n"
                    )
                elif step.base == "assets":
                    asset_path = self.assets_path / step.source
                    if asset_path.is_file():
                        content_base64 = base64.b64encode(
                            asset_path.read_bytes()
                        ).decode("utf-8")
                        docker_file_contents += (
                            f"RUN echo '{content_base64}' | base64 -d > {step.target}\n"
                        )
                    elif asset_path.is_dir():
                        raise Exception(
                            f"Asset {step.source} is a directory, shipit doesn't currently support coppying assets directories inside Docker"
                        )
                    else:
                        raise Exception(f"Asset {step.source} does not exist")
                else:
                    if step.ignore:
                        exclude = (
                            " \\\n"
                            + " \\\n".join(
                                [f"  --exclude={ignore}" for ignore in step.ignore]
                            )
                            + " \\\n "
                        )
                    else:
                        exclude = ""
                    docker_file_contents += (
                        f"COPY{exclude} {step.source} {step.target}\n"
                    )
            elif isinstance(step, EnvStep):
                env_vars = " ".join(
                    [f"{key}={value}" for key, value in step.variables.items()]
                )
                docker_file_contents += f"ENV {env_vars}\n"
                env.update(step.variables)
            elif isinstance(step, PathStep):
                docker_file_contents += f"ENV PATH={step.path}:$PATH\n"
                env["PATH"] = f"{step.path}{os.pathsep}{env.get('PATH', '')}"
            elif isinstance(step, UseStep):
                for dependency in step.dependencies:
                    if dependency.name == "pie":
                        docker_file_contents += "RUN apt-get update && apt-get -y --no-install-recommends install gcc make autoconf libtool bison re2c pkg-config libpq-dev\n"
                        docker_file_contents += "RUN curl -L --output /usr/bin/pie https://github.com/php/pie/releases/download/1.2.0/pie.phar && chmod +x /usr/bin/pie\n"
                        return
                    elif dependency.name == "static-web-server":
                        if dependency.version:
                            docker_file_contents += (
                                f"ENV SWS_INSTALL_VERSION={dependency.version}\n"
                            )
                        docker_file_contents += "RUN curl --proto '=https' --tlsv1.2 -sSfL https://get.static-web-server.net | sh\n"
                        return

                    mapped_dependency = self.mise_mapper.get(dependency.name, {})
                    package_name = mapped_dependency.get("source", dependency.name)
                    if dependency.version:
                        docker_file_contents += f"RUN mise use --global {package_name}@{dependency.version}\n"
                    else:
                        docker_file_contents += (
                            f"RUN mise use --global {package_name}\n"
                        )
                    if mapped_dependency.get("postinstall"):
                        docker_file_contents += (
                            f"RUN {mapped_dependency.get('postinstall')}\n"
                        )

        docker_file_contents += """
FROM scratch
"""
        for mount in mounts:
            docker_file_contents += (
                f"COPY --from=build {mount.build_path} {mount.build_path}\n"
            )

        self.runtime_path = env.get("PATH")

        self.docker_ignore_path.write_text("""
.shipit
Shipit
""")
        console.print(f"\n[bold]Building Docker file[/bold]")
        self.build_dockerfile(name, docker_file_contents)
        console.print(Rule(characters="-", style="bright_black"))
        console.print(f"[bold]Build complete ✅[/bold]")

    def get_runtime_path(self) -> Optional[str]:
        return self.runtime_path
