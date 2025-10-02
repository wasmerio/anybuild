from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Dict, Optional, Set
from enum import Enum
from functools import cached_property

from .base import (
    DetectResult,
    DependencySpec,
    Provider,
    _exists,
    MountSpec,
    ServiceSpec,
    VolumeSpec,
    CustomCommands,
)


class PythonFramework(Enum):
    Django = "django"
    Streamlit = "streamlit"
    FastAPI = "fastapi"
    Flask = "flask"
    FastHTML = "python-fasthtml"
    MCP = "mcp"


class PythonServer(Enum):
    Hypercorn = "hypercorn"
    Uvicorn = "uvicorn"
    # Gunicorn = "gunicorn"
    Daphne = "daphne"


class DatabaseType(Enum):
    MySQL = "mysql"
    PostgreSQL = "postgresql"


class PythonProvider:
    framework: Optional[PythonFramework] = None
    server: Optional[PythonServer] = None
    database: Optional[DatabaseType] = None
    extra_dependencies: Set[str]
    asgi_application: Optional[str] = None
    wsgi_application: Optional[str] = None
    uses_ffmpeg: bool = False
    uses_pandoc: bool = False
    only_build: bool = False
    install_requires_all_files: bool = False
    custom_commands: CustomCommands

    def __init__(
        self,
        path: Path,
        custom_commands: CustomCommands,
        only_build: bool = False,
        extra_dependencies: Optional[Set[str]] = None,
    ):
        self.path = path
        if _exists(self.path, ".python-version"):
            python_version = (self.path / ".python-version").read_text().strip()
        else:
            python_version = "3.13"
        self.default_python_version = python_version
        self.extra_dependencies = set()
        self.only_build = only_build
        self.custom_commands = custom_commands
        self.extra_dependencies = extra_dependencies or set()

        if self.only_build:
            return

        pg_deps = {
            "asyncpg",
            "aiopg",
            "psycopg",
            "psycopg2",
            "psycopg-binary",
            "psycopg2-binary",
        }
        mysql_deps = {"mysqlclient", "pymysql", "mysql-connector-python", "aiomysql", "asyncmy"}
        found_deps = self.check_deps(
            "file://",  # This is not really a dependency, but as a way to check if the install script requires all files
            "streamlit",
            "django",
            "mcp",
            "fastapi",
            "flask",
            "python-fasthtml",
            "daphne",
            "hypercorn",
            "uvicorn",
            # Other
            "ffmpeg",
            "pandoc",
            # "gunicorn",
            *mysql_deps,
            *pg_deps,
        )

        if "file://" in found_deps:
            self.install_requires_all_files = True

        # ASGI/WSGI Server
        if "uvicorn" in found_deps:
            server = PythonServer.Uvicorn
        elif "hypercorn" in found_deps:
            server = PythonServer.Hypercorn
        # elif "gunicorn" in found_deps:
        #     server = PythonServer.Gunicorn
        elif "daphne" in found_deps:
            server = PythonServer.Daphne
        else:
            server = None
        self.server = server

        if "ffmpeg" in found_deps:
            self.uses_ffmpeg = True
        if "pandoc" in found_deps:
            self.uses_pandoc = True

        if self.custom_commands.start and self.custom_commands.start.startswith("uvicorn "):
            self.server = PythonServer.Uvicorn
            self.custom_commands.start = self.custom_commands.start.replace("uvicorn ", "python -m uvicorn ")
            self.extra_dependencies = {"uvicorn"}

        # Set framework
        if _exists(self.path, "manage.py") and ("django" in found_deps):
            framework = PythonFramework.Django
            # Find the settings.py file using glob
            try:
                settings_file = next(self.path.glob("**/settings.py"))
            except StopIteration:
                settings_file = None
            if settings_file:
                asgi_match = re.search(
                    r"ASGI_APPLICATION\s*=\s*['\"](.*)['\"]", settings_file.read_text()
                )
                if asgi_match:
                    self.asgi_application = asgi_match.group(1)
                else:
                    wsgi_match = re.search(
                        r"WSGI_APPLICATION\s*=\s*['\"](.*)['\"]",
                        settings_file.read_text(),
                    )
                    if wsgi_match:
                        self.wsgi_application = wsgi_match.group(1)

            if not self.server:
                if self.asgi_application:
                    self.extra_dependencies = {"uvicorn"}
                    self.server = PythonServer.Uvicorn
                elif self.wsgi_application:
                    # gunicorn can't run with Wasmer atm
                    self.extra_dependencies = {"uvicorn"}
                    self.server = PythonServer.Uvicorn
        elif "streamlit" in found_deps:
            framework = PythonFramework.Streamlit
        elif "mcp" in found_deps:
            framework = PythonFramework.MCP
            self.extra_dependencies = {"mcp[cli]"}
        elif "fastapi" in found_deps:
            framework = PythonFramework.FastAPI
            if not self.server:
                self.extra_dependencies = {"uvicorn"}
                self.server = PythonServer.Uvicorn
        elif "flask" in found_deps:
            framework = PythonFramework.Flask
            if not self.server:
                self.extra_dependencies = {"uvicorn"}
                self.server = PythonServer.Uvicorn
        elif "python-fasthtml" in found_deps:
            framework = PythonFramework.FastHTML
        else:
            framework = None
        self.framework = framework

        # Database
        if mysql_deps & found_deps:
            database = DatabaseType.MySQL
        elif pg_deps & found_deps:
            database = DatabaseType.PostgreSQL
        else:
            database = None
        self.database = database

    def check_deps(self, *deps: str) -> Set[str]:
        deps = set([dep.lower() for dep in deps])
        initial_deps = set(deps)
        for file in ["requirements.txt", "pyproject.toml"]:
            if _exists(self.path, file):
                for line in (self.path / file).read_text().splitlines():
                    for dep in set(deps):
                        if dep in line.lower():
                            deps.remove(dep)
                            if not deps:
                                break
                    if not deps:
                        break
            if not deps:
                break
        return initial_deps - deps

    @classmethod
    def name(cls) -> str:
        return "python"

    @classmethod
    def detect(cls, path: Path, custom_commands: CustomCommands) -> Optional[DetectResult]:
        if _exists(path, "pyproject.toml", "requirements.txt"):
            if _exists(path, "manage.py"):
                return DetectResult(cls.name(), 70)
            return DetectResult(cls.name(), 50)
        if custom_commands.start:
            if custom_commands.start.startswith("python ") or custom_commands.start.startswith("uv ") or custom_commands.start.startswith("uvicorn "):
                return DetectResult(cls.name(), 80)
        return None

    def initialize(self) -> None:
        pass

    def serve_name(self) -> str:
        return self.path.name

    def provider_kind(self) -> str:
        return "python"

    def dependencies(self) -> list[DependencySpec]:
        deps = [
            DependencySpec(
                "python",
                env_var="SHIPIT_PYTHON_VERSION",
                default_version=self.default_python_version,
                use_in_build=True,
                use_in_serve=True,
            ),
            DependencySpec(
                "uv",
                env_var="SHIPIT_UV_VERSION",
                default_version="0.8.15",
                use_in_build=True,
            ),
        ]
        if self.uses_pandoc:
            deps.append(
                DependencySpec(
                    "pandoc",
                    env_var="SHIPIT_PANDOC_VERSION",
                    use_in_build=False,
                    use_in_serve=True,
                )
            )
        if self.uses_ffmpeg:
            deps.append(
                DependencySpec(
                    "ffmpeg",
                    env_var="SHIPIT_FFMPEG_VERSION",
                    use_in_build=False,
                    use_in_serve=True,
                )
            )
        return deps

    def declarations(self) -> Optional[str]:
        if self.only_build:
            return (
                'cross_platform = getenv("SHIPIT_PYTHON_CROSS_PLATFORM")\n'
                "venv = local_venv\n"
            )
        return (
            'cross_platform = getenv("SHIPIT_PYTHON_CROSS_PLATFORM")\n'
            'python_extra_index_url = getenv("SHIPIT_PYTHON_EXTRA_INDEX_URL")\n'
            'precompile_python = getenv("SHIPIT_PYTHON_PRECOMPILE") in ["true", "True", "TRUE", "1", "on", "yes", "y", "Y", "YES", "On", "ON"]\n'
            'python_cross_packages_path = venv["build"] + f"/lib/python{python_version}/site-packages"\n'
            'python_serve_site_packages_path = "{}/lib/python{}/site-packages".format(venv["serve"], python_version)\n'
            'app_serve_path = app["serve"]\n'
        )

    def build_steps(self) -> list[str]:
        if not self.only_build:
            steps = ['workdir(app["build"])']
        else:
            steps = []

        extra_deps = ", ".join([f"{dep}" for dep in self.extra_dependencies])
        has_requirements = _exists(self.path, "requirements.txt")
        if _exists(self.path, "pyproject.toml"):
            input_files = ["pyproject.toml"]
            extra_args = ""
            if _exists(self.path, "uv.lock"):
                input_files.append("uv.lock")
                extra_args = " --locked"

            # Extra input files check, as glob pattern
            globs = ["README*", "LICENSE*", "LICENCE*", "MAINTAINERS*", "AUTHORS*"]
            # Glob check
            for glob in globs:
                for path in self.path.glob(glob):
                    # make path relative to self.path
                    path = str(path.relative_to(self.path))
                    input_files.append(path)

            # Join inputs
            inputs = ", ".join([f'"{input}"' for input in input_files])
            steps += [
                'env(UV_PROJECT_ENVIRONMENT=local_venv["build"] if cross_platform else venv["build"])',
                'copy(".", ".")' if self.install_requires_all_files else None,
                f'run(f"uv sync --compile --python python{{python_version}} --no-managed-python{extra_args}", inputs=[{inputs}], group="install")',
                'copy("pyproject.toml", "pyproject.toml")'
                if not self.install_requires_all_files
                else None,
                f'run("uv add {extra_deps}", group="install")' if extra_deps else None,
            ]
            if not self.only_build:
                steps += [
                    'run(f"uv pip compile pyproject.toml --python-version={python_version} --universal --extra-index-url {python_extra_index_url} --index-url=https://pypi.org/simple --emit-index-url -o cross-requirements.txt", outputs=["cross-requirements.txt"]) if cross_platform else None',
                    f'run(f"uvx pip install -r cross-requirements.txt {extra_deps} --target {{python_cross_packages_path}} --platform {{cross_platform}} --only-binary=:all: --python-version={{python_version}} --compile") if cross_platform else None',
                    'run("rm cross-requirements.txt") if cross_platform else None',
                ]
        elif has_requirements or extra_deps:
            steps += [
                'env(UV_PROJECT_ENVIRONMENT=local_venv["build"] if cross_platform else venv["build"])',
                'run(f"uv init --no-workspace --no-managed-python --python python{python_version}", inputs=[], outputs=["uv.lock"], group="install")',
                'copy(".", ".")' if self.install_requires_all_files else None,
            ]
            if has_requirements:
                steps += [
                    f'run("uv add -r requirements.txt {extra_deps}", inputs=["requirements.txt"], group="install")',
                ]
            else:
                steps += [
                    f'run("uv add {extra_deps}", group="install")',
                ]
            if not self.only_build:
                steps += [
                    'run(f"uv pip compile requirements.txt --python-version={python_version} --universal --extra-index-url {python_extra_index_url} --index-url=https://pypi.org/simple --emit-index-url -o cross-requirements.txt", inputs=["requirements.txt"], outputs=["cross-requirements.txt"]) if cross_platform else None',
                    f'run(f"uvx pip install -r cross-requirements.txt {extra_deps} --target {{python_cross_packages_path}} --platform {{cross_platform}} --only-binary=:all: --python-version={{python_version}} --compile") if cross_platform else None',
                    'run("rm cross-requirements.txt") if cross_platform else None',
                ]

        steps += [
            'path((local_venv["build"] if cross_platform else venv["build"]) + "/bin")',
            'copy(".", ".", ignore=[".venv", ".git", "__pycache__"])',
        ]
        if self.framework == PythonFramework.MCP:
            steps += [
                'run("mkdir -p {}/bin".format(venv["build"])) if cross_platform else None',
                'run("cp {}/bin/mcp {}/bin/mcp".format(local_venv["build"], venv["build"])) if cross_platform else None',
            ]
        return list(filter(None, steps))

    def prepare_steps(self) -> Optional[list[str]]:
        if self.only_build:
            return []
        return [
            'run("echo \\"Precompiling Python code...\\"") if precompile_python else None',
            'run(f"python -m compileall -o 2 {python_serve_site_packages_path}") if precompile_python else None',
            'run("echo \\"Precompiling package code...\\"") if precompile_python else None',
            'run(f"python -m compileall -o 2 {app_serve_path}") if precompile_python else None',
        ]

    @cached_property
    def main_file(self) -> Optional[str]:
        paths_to_try = ["main.py", "app.py", "streamlit_app.py", "Home.py", "*_app.py"]
        for path in paths_to_try:
            if "*" in path:
                continue  # This is for the glob finder
            if _exists(self.path, path):
                return path
            if _exists(self.path, f"src/{path}"):
                return f"src/{path}"
        for path in paths_to_try:
            try:
                found_path = next(self.path.glob(f"**/{path}"))
            except StopIteration:
                found_path = None
            if found_path:
                return str(found_path.relative_to(self.path))
        return None

    def commands(self) -> Dict[str, str]:
        commands = self.base_commands()
        if self.custom_commands.start:
            commands["start"] = json.dumps(self.custom_commands.start)
        return commands

    def base_commands(self) -> Dict[str, str]:
        if self.only_build:
            return {}
        if self.framework == PythonFramework.Django:
            start_cmd = None
            if self.server == PythonServer.Daphne and self.asgi_application:
                asgi_application = format_app_import(self.asgi_application)
                start_cmd = (
                    f'"python -m daphne {asgi_application} --bind 0.0.0.0 --port 8000"'
                )
            elif self.server == PythonServer.Uvicorn:
                if self.asgi_application:
                    asgi_application = format_app_import(self.asgi_application)
                    start_cmd = f'"python -m uvicorn {asgi_application} --host 0.0.0.0 --port 8000"'
                elif self.wsgi_application:
                    wsgi_application = format_app_import(self.wsgi_application)
                    start_cmd = f'"python -m uvicorn {wsgi_application} --interface=wsgi --host 0.0.0.0 --port 8000"'
            # elif self.server == PythonServer.Gunicorn:
            #     start_cmd = f'"python -m gunicorn {self.wsgi_application} --bind 0.0.0.0 --port 8000"'
            if not start_cmd:
                # We run the default runserver command if no server is specified
                start_cmd = '"python manage.py runserver 0.0.0.0:8000"'
            migrate_cmd = '"python manage.py migrate"'
            return {"start": start_cmd, "after_deploy": migrate_cmd}

        main_file = self.main_file

        if not main_file:
            start_cmd = '"python -c \'print(\\"No start command detected, please provide a start command manually\\")\'"'
            return {"start": start_cmd}

        if self.framework == PythonFramework.FastAPI:
            python_path = file_to_python_path(main_file)
            path = f"{python_path}:app"
            if self.server == PythonServer.Uvicorn:
                start_cmd = f'"python -m uvicorn {path} --host 0.0.0.0 --port 8000"'
            elif self.server == PythonServer.Hypercorn:
                start_cmd = f'"python -m hypercorn {path} --bind 0.0.0.0:8000"'
            else:
                start_cmd = '"python -c \'print(\\"No start command detected, please provide a start command manually\\")\'"'
            return {"start": start_cmd}

        elif self.framework == PythonFramework.Streamlit:
            start_cmd = f'"python -m streamlit run {main_file} --server.port 8000 --server.address 0.0.0.0 --server.headless true"'

        elif self.framework == PythonFramework.Flask:
            python_path = file_to_python_path(main_file)
            path = f"{python_path}:app"
            # start_cmd = f'"python -m flask --app {path} run --debug --host 0.0.0.0 --port 8000"'
            start_cmd = f'"python -m uvicorn {path} --interface=wsgi --host 0.0.0.0 --port 8000"'

        elif self.framework == PythonFramework.MCP:
            contents = (self.path / main_file).read_text()
            if 'if __name__ == "__main__"' in contents or "mcp.run" in contents:
                start_cmd = f'"python {main_file}"'
            else:
                start_cmd = f'"python {{}}/bin/mcp run {main_file} --transport=streamable-http".format(venv["serve"])'

        elif self.framework == PythonFramework.FastHTML:
            python_path = file_to_python_path(main_file)
            path = f"{python_path}:app"
            start_cmd = f'"python -m uvicorn {path} --host 0.0.0.0 --port 8000"'

        else:
            start_cmd = f'"python {main_file}"'

        return {"start": start_cmd}

    def mounts(self) -> list[MountSpec]:
        if self.only_build:
            return [
                MountSpec("local_venv", attach_to_serve=False),
            ]
        return [
            MountSpec("app"),
            MountSpec("venv"),
            MountSpec("local_venv", attach_to_serve=False),
        ]

    def volumes(self) -> list[VolumeSpec]:
        return []

    def env(self) -> Optional[Dict[str, str]]:
        if self.only_build:
            return {}
        # For Django projects, generate an empty env dict to surface the field
        # in the Shipit file. Other Python projects omit it by default.
        python_path = "f\"{app_serve_path}:{python_serve_site_packages_path}\""
        main_file = self.main_file
        if main_file and main_file.startswith("src/"):
            python_path = "f\"{app_serve_path}:{app_serve_path}/src:{python_serve_site_packages_path}\""
        else:
            python_path =  "f\"{app_serve_path}:{python_serve_site_packages_path}\""
        env_vars = {"PYTHONPATH": python_path, "HOME": 'app["serve"]'}
        if self.framework == PythonFramework.Streamlit:
            env_vars["STREAMLIT_SERVER_HEADLESS"] = '"true"'
        elif self.framework == PythonFramework.MCP:
            env_vars["FASTMCP_HOST"] = '"0.0.0.0"'
            env_vars["FASTMCP_PORT"] = '"8000"'
        return env_vars
    
    def services(self) -> list[ServiceSpec]:
        if self.database == DatabaseType.MySQL:
            return [ServiceSpec(name="database", provider="mysql")]
        elif self.database == DatabaseType.PostgreSQL:
            return [ServiceSpec(name="database", provider="postgres")]
        return []


def format_app_import(asgi_application: str) -> str:
    # Transform "mysite.asgi.application" to "mysite.asgi:application" using regex
    return re.sub(r"\.([^.]+)$", r":\1", asgi_application)


def file_to_python_path(path: str) -> str:
    return path.rstrip(".py").replace("/", ".").replace("\\", ".")
