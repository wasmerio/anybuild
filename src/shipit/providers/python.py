from __future__ import annotations

import re
from pathlib import Path
from typing import Dict, Optional, Set
from enum import Enum

from .base import (
    DetectResult,
    DependencySpec,
    Provider,
    _exists,
    MountSpec,
)


class PythonFramework(Enum):
    Django = "django"
    FastAPI = "fastapi"
    Flask = "flask"
    FastHTML = "python-fasthtml"


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

    def __init__(self, path: Path):
        self.path = path
        if _exists(self.path, ".python-version"):
            python_version = (self.path / ".python-version").read_text().strip()
        else:
            python_version = "3.13"
        self.default_python_version = python_version
        self.extra_dependencies = set()

        pg_deps = {
            "asyncpg",
            "aiopg",
            "psycopg",
            "psycopg2",
            "psycopg-binary",
            "psycopg2-binary"}
        mysql_deps = {"mysqlclient", "pymysql", "mysql-connector-python", "aiomysql"}
        found_deps = self.check_deps(
            "django",
            "fastapi",
            "flask",
            "python-fasthtml",
            "daphne",
            "hypercorn",
            "uvicorn",
            # "gunicorn",
            *mysql_deps,
            *pg_deps,
        )

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

        # Set framework
        if _exists(self.path, "manage.py") and ("django" in found_deps):
            framework = PythonFramework.Django
            # Find the settings.py file using glob
            settings_file = next(self.path.glob( "**/settings.py"))
            if settings_file:
                asgi_match = re.search(r"ASGI_APPLICATION\s*=\s*['\"](.*)['\"]", settings_file.read_text())
                if asgi_match:
                    self.asgi_application = asgi_match.group(1)
                else:
                    wsgi_match = re.search(r"WSGI_APPLICATION\s*=\s*['\"](.*)['\"]", settings_file.read_text())
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
        elif "fastapi" in found_deps:
            framework = PythonFramework.FastAPI
            if not self.server:
                self.extra_dependencies = {"uvicorn"}
                self.server = PythonServer.Uvicorn
        elif "flask" in found_deps:
            framework = PythonFramework.Flask
        elif "fastapi" in found_deps:
            framework = PythonFramework.FastAPI
        elif "flask" in found_deps:
            framework = PythonFramework.Flask
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
        return initial_deps-deps

    @classmethod
    def name(cls) -> str:
        return "python"

    @classmethod
    def detect(cls, path: Path) -> Optional[DetectResult]:
        if _exists(path, "pyproject.toml", "requirements.txt"):
            if _exists(path, "manage.py"):
                return DetectResult(cls.name(), 70)
            return DetectResult(cls.name(), 50)
        return None

    def initialize(self) -> None:
        pass

    def serve_name(self) -> str:
        return self.path.name

    def provider_kind(self) -> str:
        return "python"

    def dependencies(self) -> list[DependencySpec]:
        return [
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

    def declarations(self) -> Optional[str]:
        return (
            "cross_platform = getenv(\"SHIPIT_PYTHON_CROSS_PLATFORM\")\n"
            "python_extra_index_url = getenv(\"SHIPIT_PYTHON_EXTRA_INDEX_URL\")\n"
            "precompile_python = getenv(\"SHIPIT_PYTHON_PRECOMPILE\") in [\"true\", \"True\", \"TRUE\", \"1\", \"on\", \"yes\", \"y\", \"Y\", \"YES\", \"On\", \"ON\"]\n"
            "python_cross_packages_path = venv[\"build\"] + f\"/lib/python{python_version}/site-packages\"\n"
            "python_serve_path = \"{}/lib/python{}/site-packages\".format(venv[\"serve\"], python_version)\n"
        )

    def build_steps(self) -> list[str]:
        steps = [
            "workdir(app[\"build\"])"
        ]

        if _exists(self.path, "pyproject.toml"):
            input_files = ["pyproject.toml"]
            extra_args = ""
            if _exists(self.path, "uv.lock"):
                input_files.append("uv.lock")
                extra_args = " --locked"
            inputs = ", ".join([f"\"{input}\"" for input in input_files])
            extra_deps = ", ".join([f"{dep}" for dep in self.extra_dependencies])
            steps += list(filter(None, [
                "env(UV_PROJECT_ENVIRONMENT=local_venv[\"build\"] if cross_platform else venv[\"build\"])",
                "run(f\"uv sync --compile --python python{python_version} --no-managed-python" + extra_args + "\", inputs=[" + inputs + "], group=\"install\")",
                "copy(\"pyproject.toml\", \"pyproject.toml\")",
                f"run(\"uv add {extra_deps}\", group=\"install\")" if extra_deps else None,
                "run(f\"uv pip compile pyproject.toml --python-version={python_version} --universal --extra-index-url {python_extra_index_url} --index-url=https://pypi.org/simple --emit-index-url --only-binary :all: -o cross-requirements.txt\", outputs=[\"cross-requirements.txt\"]) if cross_platform else None",
                f"run(f\"uvx pip install -r cross-requirements.txt {extra_deps} --target {{python_cross_packages_path}} --platform {{cross_platform}} --only-binary=:all: --python-version={{python_version}} --compile\") if cross_platform else None",
                "run(\"rm cross-requirements.txt\") if cross_platform else None",
            ]))
        if _exists(self.path, "requirements.txt"):
            steps += [
                "env(UV_PROJECT_ENVIRONMENT=local_venv[\"build\"] if cross_platform else venv[\"build\"])",
                "run(f\"uv init --no-managed-python --python python{python_version}\", inputs=[], outputs=[\"uv.lock\"], group=\"install\")",
                "run(f\"uv add -r requirements.txt\", inputs=[\"requirements.txt\"], group=\"install\")",
                "run(f\"uv pip compile requirements.txt --python-version={python_version} --universal --extra-index-url {python_extra_index_url} --index-url=https://pypi.org/simple --emit-index-url --only-binary :all: -o cross-requirements.txt\", inputs=[\"requirements.txt\"], outputs=[\"cross-requirements.txt\"]) if cross_platform else None",
                "run(f\"uvx pip install -r cross-requirements.txt --target {python_cross_packages_path} --platform {cross_platform} --only-binary=:all: --python-version={python_version} --compile\") if cross_platform else None",
                "run(\"rm cross-requirements.txt\") if cross_platform else None",
            ]

        steps += [
            "path((local_venv[\"build\"] if cross_platform else venv[\"build\"]) + \"/bin\")",
            "copy(\".\", \".\", ignore=[\".venv\", \".git\", \"__pycache__\"])",
        ]
        return steps

    def prepare_steps(self) -> Optional[list[str]]:
        return [
            'run("echo \\\"Precompiling Python code...\\\"") if precompile_python else None',
            'run(f"python -m compileall -o 2 {python_serve_path}") if precompile_python else None',
            'run("echo \\\"Precompiling package code...\\\"") if precompile_python else None',
            'run("python -m compileall -o 2 {}".format(app["serve"])) if precompile_python else None',
        ]

    def commands(self) -> Dict[str, str]:
        if self.framework == PythonFramework.Django:
            start_cmd = None
            if self.server == PythonServer.Daphne and self.asgi_application:
                asgi_application = format_app_import(self.asgi_application)
                start_cmd = f'"python -m daphne {asgi_application} --bind 0.0.0.0 --port 8000"'
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
        elif self.framework == PythonFramework.FastAPI:
            if _exists(self.path, "main.py"):
                path = "main:app"
            elif _exists(self.path, "src/main.py"):
                path = "src.main:app"
            
            if self.server == PythonServer.Uvicorn:
                start_cmd = f'"python -m uvicorn {path} --host 0.0.0.0 --port 8000"'
            elif self.server == PythonServer.Hypercorn:
                start_cmd = f'"python -m hypercorn {path} --bind 0.0.0.0:8000"'
            else:
                start_cmd = '"python -c \'print(\\\"No start command detected, please provide a start command manually\\\")\'"'
            return {"start": start_cmd}
        elif self.framework == PythonFramework.FastHTML:
            if _exists(self.path, "main.py"):
                path = "main:app"
            elif _exists(self.path, "src/main.py"):
                path = "src.main:app"
            start_cmd = f'"python -m uvicorn {path} --host 0.0.0.0 --port 8000"'
        elif _exists(self.path, "main.py"):
            start_cmd = '"python main.py"'
        elif _exists(self.path, "src/main.py"):
            start_cmd = '"python src/main.py"'
        else:
            start_cmd = '"python -c \'print(\\\"No start command detected, please provide a start command manually\\\")\'"'
        return {"start": start_cmd}

    def assets(self) -> Optional[Dict[str, str]]:
        return None

    def mounts(self) -> list[MountSpec]:
        return [
            MountSpec("app"),
            MountSpec("venv"),
            MountSpec("local_venv", attach_to_serve=False),
        ]

    def env(self) -> Optional[Dict[str, str]]:
        # For Django projects, generate an empty env dict to surface the field
        # in the Shipit file. Other Python projects omit it by default.
        return {
            "PYTHONPATH": "python_serve_path"
        }

def format_app_import(asgi_application: str) -> str:
    # Transform "mysite.asgi.application" to "mysite.asgi:application" using regex
    return re.sub(r"\.([^.]+)$", r":\1", asgi_application)
