import re
from pathlib import Path
from typing import Dict, Optional, Set
from enum import Enum

from pydantic import Field
from pydantic_settings import SettingsConfigDict

from .base import (
    DetectResult,
    DependencySpec,
    Provider,
    _exists,
    MountSpec,
    ServiceSpec,
    VolumeSpec,
    Config,
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


class PythonConfig(Config):
    model_config = SettingsConfigDict(
        extra="ignore", env_prefix="SHIPIT_"
    )

    framework: Optional[PythonFramework] = None
    server: Optional[PythonServer] = None
    database: Optional[DatabaseType] = None
    extra_dependencies: Set[str] = Field(default_factory=set)
    asgi_application: Optional[str] = None
    wsgi_application: Optional[str] = None
    uses_ffmpeg: bool = False
    uses_pandoc: bool = False
    install_requires_all_files: bool = False
    main_file: Optional[str] = None
    python_version: Optional[str] = "3.13"
    uv_version: Optional[str] = "0.8.15"
    precompile_python: bool = True
    cross_platform: Optional[str] = None
    python_extra_index_url: Optional[str] = None
    pandoc_version: Optional[str] = None
    ffmpeg_version: Optional[str] = None


class PythonProvider:
    only_build: bool = False

    def __init__(
        self,
        path: Path,
        config: PythonConfig,
        only_build: bool = False,
    ):
        self.path = path
        self.config = config
        self.only_build = only_build

    @classmethod
    def load_config(
        cls,
        path: Path,
        base_config: Config,
        must_have_deps: Optional[Set[str]] = None,
    ) -> PythonConfig:
        config = PythonConfig(**base_config.model_dump())

        if not config.main_file:
            config.main_file = cls.detect_main_file(path)

        if not config.python_version:
            if _exists(path, ".python-version"):
                python_version = (path / ".python-version").read_text().strip()
            else:
                python_version = "3.13"
            config.python_version = python_version

        pg_deps = {
            "asyncpg",
            "aiopg",
            "psycopg",
            "psycopg2",
            "psycopg-binary",
            "psycopg2-binary",
        }
        mysql_deps = {
            "mysqlclient",
            "pymysql",
            "mysql-connector-python",
            "aiomysql",
            "asyncmy",
        }
        must_have_deps = must_have_deps or set()
        found_deps = cls.check_deps(
            path,
            "file://",  # This is not really a dependency, but as a way to check if the install script requires all files
            "streamlit",
            "django",
            "mcp",
            "mcp[cli]",
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
            *must_have_deps,
        )

        if "file://" in found_deps:
            config.install_requires_all_files = True

        if not config.server:
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
            config.server = server

        if "ffmpeg" in found_deps:
            config.uses_ffmpeg = True
        if "pandoc" in found_deps:
            config.uses_pandoc = True

        if not config.framework:
            # Set framework
            if _exists(path, "manage.py") and ("django" in found_deps):
                framework = PythonFramework.Django
            elif "streamlit" in found_deps:
                framework = PythonFramework.Streamlit
            elif "mcp" in found_deps:
                framework = PythonFramework.MCP
            elif "fastapi" in found_deps:
                framework = PythonFramework.FastAPI
            elif "flask" in found_deps:
                framework = PythonFramework.Flask
            elif "python-fasthtml" in found_deps:
                framework = PythonFramework.FastHTML
            else:
                framework = None
            config.framework = framework

        if not config.server and config.framework:
            if config.framework == PythonFramework.Django:
                config.server = PythonServer.Uvicorn
            elif config.framework == PythonFramework.FastAPI:
                config.server = PythonServer.Uvicorn
            elif config.framework == PythonFramework.Flask:
                config.server = PythonServer.Uvicorn
            elif config.framework == PythonFramework.FastHTML:
                config.server = PythonServer.Uvicorn

            if config.server == PythonServer.Uvicorn:
                must_have_deps.add("uvicorn")

        if not config.asgi_application and not config.wsgi_application:
            if config.framework == PythonFramework.Django:
                # Find the settings.py file using glob
                try:
                    settings_file = next(path.glob("**/settings.py"))
                except StopIteration:
                    settings_file = None
                if settings_file:
                    asgi_match = re.search(
                        r"ASGI_APPLICATION\s*=\s*['\"](.*)['\"]",
                        settings_file.read_text(),
                    )
                    if asgi_match:
                        config.asgi_application = format_app_import(
                            asgi_match.group(1)
                        )
                    else:
                        wsgi_match = re.search(
                            r"WSGI_APPLICATION\s*=\s*['\"](.*)['\"]",
                            settings_file.read_text(),
                        )
                        if wsgi_match:
                            config.wsgi_application = format_app_import(
                                wsgi_match.group(1)
                            )

            python_path = file_to_python_path(config.main_file)
            if config.framework == PythonFramework.FastAPI:
                config.asgi_application = python_path
            elif config.framework == PythonFramework.Flask:
                config.wsgi_application = python_path
            elif config.framework == PythonFramework.MCP:
                config.asgi_application = python_path
            elif config.framework == PythonFramework.FastHTML:
                config.asgi_application = python_path

        is_uvicorn_start = config.commands.start and config.commands.start.startswith(
            "uvicorn "
        )
        framework_should_use_uvicorn = config.framework in [
            PythonFramework.Django,
            PythonFramework.FastAPI,
            PythonFramework.Flask,
        ]
        if is_uvicorn_start or (framework_should_use_uvicorn and not config.server):
            must_have_deps.add("uvicorn")
            config.server = PythonServer.Uvicorn
        if config.framework == PythonFramework.MCP:
            must_have_deps.add("mcp[cli]")

        for dep in must_have_deps:
            if dep not in found_deps:
                config.extra_dependencies.add(dep)

        if not config.database:
            # Database
            if mysql_deps & found_deps:
                database = DatabaseType.MySQL
            elif pg_deps & found_deps:
                database = DatabaseType.PostgreSQL
            else:
                database = None
            config.database = database

        return config

    @classmethod
    def check_deps(cls, path: Path, *deps: str) -> Set[str]:
        deps = set([dep.lower() for dep in deps])
        initial_deps = set(deps)
        for file in ["requirements.txt", "pyproject.toml"]:
            if _exists(path, file):
                for line in (path / file).read_text().splitlines():
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
    def detect(
        cls, path: Path, config: Config
    ) -> Optional[DetectResult]:
        if _exists(path, "pyproject.toml", "requirements.txt"):
            if _exists(path, "manage.py"):
                return DetectResult(cls.name(), 70)
            return DetectResult(cls.name(), 50)
        if config.commands.start:
            if (
                config.commands.start.startswith("python ")
                or config.commands.start.startswith("uv ")
                or config.commands.start.startswith("uvicorn ")
                or config.commands.start.startswith("gunicorn ")
            ):
                return DetectResult(cls.name(), 80)
        if cls.detect_main_file(path):
            return DetectResult(cls.name(), 10)
        return None

    def serve_name(self) -> Optional[str]:
        return None

    def dependencies(self) -> list[DependencySpec]:
        deps = [
            DependencySpec(
                "python",
                var_name="config.python_version",
                use_in_build=True,
                use_in_serve=True,
            ),
            DependencySpec(
                "uv",
                var_name="config.uv_version",
                use_in_build=True,
            ),
        ]
        if self.config.uses_pandoc:
            deps.append(
                DependencySpec(
                    "pandoc",
                    var_name="config.pandoc_version",
                    use_in_build=False,
                    use_in_serve=True,
                )
            )
        if self.config.uses_ffmpeg:
            deps.append(
                DependencySpec(
                    "ffmpeg",
                    var_name="config.ffmpeg_version",
                    use_in_build=False,
                    use_in_serve=True,
                )
            )
        return deps

    def declarations(self) -> Optional[str]:
        if self.only_build:
            return (
                "python_version = config.python_version\n"
                "cross_platform = config.cross_platform\n"
                "venv = local_venv\n"
            )
        return (
            "python_version = config.python_version\n"
            "cross_platform = config.cross_platform\n"
            "python_extra_index_url = config.python_extra_index_url\n"
            "precompile_python = config.precompile_python\n"
            'python_cross_packages_path = venv.path + f"/lib/python{python_version}/site-packages"\n'
            'python_serve_site_packages_path = "{}/lib/python{}/site-packages".format(venv.serve_path, python_version)\n'
            'app_serve_path = app.serve_path\n'
        )

    def build_steps(self) -> list[str]:
        if not self.only_build:
            steps = ['workdir(app.path)']
        else:
            steps = ['workdir(temp.path)']

        extra_deps = ", ".join([f"{dep}" for dep in self.config.extra_dependencies])
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
                'env(UV_PROJECT_ENVIRONMENT=local_venv.path if cross_platform else venv.path, UV_PYTHON_PREFERENCE="only-system", UV_PYTHON=f"python{python_version}")',
                'copy(".", ".")' if self.config.install_requires_all_files else None,
                f'run(f"uv sync{extra_args}", inputs=[{inputs}], group="install")',
                'copy("pyproject.toml", "pyproject.toml")'
                if not self.config.install_requires_all_files
                else None,
                f'run("uv add {extra_deps}", group="install")' if extra_deps else None,
            ]
            if not self.only_build:
                steps += [
                    'run(f"uv pip compile pyproject.toml --universal --extra-index-url {python_extra_index_url} --index-url=https://pypi.org/simple --emit-index-url --no-deps -o cross-requirements.txt", outputs=["cross-requirements.txt"]) if cross_platform else None',
                    f'run(f"uvx pip install -r cross-requirements.txt {extra_deps} --target {{python_cross_packages_path}} --platform {{cross_platform}} --only-binary=:all: --python-version={{python_version}} --compile") if cross_platform else None',
                    'run("rm cross-requirements.txt") if cross_platform else None',
                ]
        elif has_requirements or extra_deps:
            steps += [
                'env(UV_PROJECT_ENVIRONMENT=local_venv.path if cross_platform else venv.path)',
                'run(f"uv init", inputs=[], outputs=["uv.lock"], group="install")',
                'copy(".", ".", ignore=[".venv", ".git", "__pycache__"])'
                if self.config.install_requires_all_files
                else None,
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
                    'run(f"uv pip compile requirements.txt --python-version={python_version} --universal --extra-index-url {python_extra_index_url} --index-url=https://pypi.org/simple --emit-index-url --no-deps -o cross-requirements.txt", inputs=["requirements.txt"], outputs=["cross-requirements.txt"]) if cross_platform else None',
                    f'run(f"uvx pip install -r cross-requirements.txt {extra_deps} --target {{python_cross_packages_path}} --platform {{cross_platform}} --only-binary=:all: --python-version={{python_version}} --compile") if cross_platform else None',
                    'run("rm cross-requirements.txt") if cross_platform else None',
                ]

        steps += [
            'path((local_venv.path if cross_platform else venv.path) + "/bin")',
            'copy(".", ".", ignore=[".venv", ".git", "__pycache__"])'
            if not self.config.install_requires_all_files
            else None,
        ]
        if self.config.framework == PythonFramework.MCP:
            steps += [
                'run("mkdir -p {}/bin".format(venv.path)) if cross_platform else None',
                'run("cp {}/bin/mcp {}/bin/mcp".format(local_venv.path, venv.path)) if cross_platform else None',
            ]
        if self.config.framework == PythonFramework.Django:
            steps += [
                'run("python manage.py collectstatic --noinput", group="build")',
            ]
        return list(filter(None, steps))

    def prepare_steps(self) -> Optional[list[str]]:
        if self.only_build:
            return []
        return [
            'run("echo \\"Precompiling Python code...\\"") if precompile_python else None',
            'run(f"python -m compileall -o 2 {python_serve_site_packages_path} || true") if precompile_python else None',
            'run("echo \\"Precompiling package code...\\"") if precompile_python else None',
            'run(f"python -m compileall -o 2 {app_serve_path} || true") if precompile_python else None',
        ]

    @classmethod
    def detect_main_file(cls, root_path: Path) -> Optional[str]:
        paths_to_try = ["main.py", "app.py", "streamlit_app.py", "Home.py", "*_app.py"]
        for path in paths_to_try:
            if "*" in path:
                continue  # This is for the glob finder
            if _exists(root_path, path):
                return path
            if _exists(root_path, f"src/{path}"):
                return f"src/{path}"
        for path in paths_to_try:
            found_path = next(root_path.glob(f"*/{path}"), None)
            if not found_path:
                found_path = next(root_path.glob(f"*/*/{path}"), None)
            if found_path:
                return str(found_path.relative_to(root_path))
        return None

    def commands(self) -> Dict[str, str]:
        return self.base_commands()

    def base_commands(self) -> Dict[str, str]:
        if self.only_build:
            return {}

        start_cmd = None
        if self.config.server == PythonServer.Daphne:
            assert self.config.asgi_application, (
                "No ASGI application found for Daphne"
            )
            start_cmd = f'f"daphne {self.config.asgi_application} --bind 0.0.0.0 --port {{PORT}}"'
        # elif self.config.server == PythonServer.Gunicorn:
        #     assert self.config.wsgi_application, "No WSGI application found"
        #     start_cmd = f'f"gunicorn {self.config.wsgi_application} --bind 0.0.0.0 --port {{PORT}}"'
        elif self.config.server == PythonServer.Uvicorn:
            if not self.config.main_file:
                assert (
                    self.config.asgi_application or self.config.wsgi_application
                ), (
                    "No ASGI or WSGI application found for Uvicorn and no main file found"
                )
            if self.config.asgi_application:
                start_cmd = f'f"uvicorn {self.config.asgi_application} --host 0.0.0.0 --port {{PORT}}"'
            elif self.config.wsgi_application:
                start_cmd = f'f"uvicorn {self.config.wsgi_application} --interface=wsgi --host 0.0.0.0 --port {{PORT}}"'
        elif self.config.server == PythonServer.Hypercorn:
            assert self.config.asgi_application, (
                "No ASGI application found for Hypercorn"
            )
            start_cmd = (
                f'f"hypercorn {self.config.asgi_application} --bind 0.0.0.0:{{PORT}}"'
            )
        elif self.config.framework == PythonFramework.Streamlit:
            assert self.config.main_file, "No main file found for Streamlit"
            main_file = self.config.main_file
            start_cmd = f'f"streamlit run {main_file} --server.port {{PORT}} --server.address 0.0.0.0 --server.headless true"'
        elif self.config.framework == PythonFramework.MCP:
            main_file = self.config.main_file
            assert main_file, "No main file found for MCP"
            contents = (self.path / main_file).read_text()
            if 'if __name__ == "__main__"' in contents or "mcp.run" in contents:
                start_cmd = f'"python {main_file}"'
            else:
                start_cmd = f'"python {{}}/bin/mcp run {main_file} --transport=streamable-http".format(venv.serve_path)'

        if not start_cmd:
            if self.config.main_file:
                start_cmd = f'"python {self.config.main_file}"'

        if self.config.framework == PythonFramework.Django:
            if not start_cmd:
                start_cmd = 'f"python manage.py runserver 0.0.0.0:{PORT}"'
            migrate_cmd = '"python manage.py migrate"'
            return {"start": start_cmd, "after_deploy": migrate_cmd}

        if start_cmd:
            return {"start": start_cmd}
        else:
            return {}

    def mounts(self) -> list[MountSpec]:
        if self.only_build:
            return [
                MountSpec("temp", attach_to_serve=False),
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
        python_path = 'f"{app_serve_path}:{python_serve_site_packages_path}"'
        main_file = self.config.main_file
        if main_file and main_file.startswith("src/"):
            python_path = 'f"{app_serve_path}:{app_serve_path}/src:{python_serve_site_packages_path}"'
        else:
            python_path = 'f"{app_serve_path}:{python_serve_site_packages_path}"'
        env_vars = {"PYTHONPATH": python_path, "HOME": 'app.serve_path'}
        if self.config.framework == PythonFramework.Streamlit:
            env_vars["STREAMLIT_SERVER_HEADLESS"] = '"true"'
        elif self.config.framework == PythonFramework.MCP:
            env_vars["FASTMCP_HOST"] = '"0.0.0.0"'
            env_vars["FASTMCP_PORT"] = "PORT"
        return env_vars

    def services(self) -> list[ServiceSpec]:
        if self.config.database == DatabaseType.MySQL:
            return [ServiceSpec(name="database", provider="mysql")]
        elif self.config.database == DatabaseType.PostgreSQL:
            return [ServiceSpec(name="database", provider="postgres")]
        return []


def format_app_import(asgi_application: str) -> str:
    # Transform "mysite.asgi.application" to "mysite.asgi:application" using regex
    return re.sub(r"\.([^.]+)$", r":\1", asgi_application)


def file_to_python_path(path: Optional[str]) -> Optional[str]:
    if not path:
        return None
    file = path.rstrip(".py").replace("/", ".").replace("\\", ".")
    return f"{file}:app"
