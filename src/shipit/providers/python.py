from enum import Enum
import re
from pathlib import Path
from typing import List, Optional, Set

from pydantic import Field
from pydantic_settings import SettingsConfigDict

from .install_context import discover_python_dependency_files, discover_python_install_context
from .base import DetectResult, _exists, Config


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


class MigrationStrategy(Enum):
    Django = "django"
    Alembic = "alembic"



class DatabaseType(Enum):
    MySQL = "mysql"
    PostgreSQL = "postgresql"


class PythonConfig(Config):
    model_config = SettingsConfigDict(
        extra="ignore", env_prefix="SHIPIT_"
    )

    framework: Optional[PythonFramework] = None
    server: Optional[PythonServer] = None
    migration_strategy: Optional[MigrationStrategy] = None
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
    # Derived install inputs for the Starlark provider (None => all files).
    install_inputs: Optional[List[str]] = None
    # MCP main file starts its own server (mcp.run()/__main__ block).
    mcp_self_running: bool = False


class PythonProvider:
    def __init__(self, path: Path, config: PythonConfig):
        self.path = path
        self.config = config

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
            "mariadb",
        }
        must_have_deps = must_have_deps or set()
        found_deps = cls.check_deps(
            path,
            "file://",  # This is not really a dependency, but as a way to check if the install script requires all files
            "streamlit",
            "alembic",
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

        dependency_context = discover_python_install_context(
            path,
            include_pyproject=_exists(path, "pyproject.toml"),
            include_requirements=_exists(path, "requirements.txt"),
        )
        if dependency_context.requires_all_files:
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
                config.migration_strategy = MigrationStrategy.Django
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

        if not config.migration_strategy:
            if config.framework == PythonFramework.Django:
                config.migration_strategy = MigrationStrategy.Django
            elif "alembic" in found_deps or _exists(path, "alembic.ini"):
                config.migration_strategy = MigrationStrategy.Alembic

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

        if _exists(path, "pyproject.toml"):
            config.install_inputs = discover_python_install_context(
                path, include_pyproject=True
            ).inputs
        else:
            config.install_inputs = discover_python_install_context(
                path, include_requirements=_exists(path, "requirements.txt")
            ).inputs
        if config.framework == PythonFramework.MCP and config.main_file:
            main_path = path / config.main_file
            if main_path.is_file():
                contents = main_path.read_text()
                config.mcp_self_running = (
                    'if __name__ == "__main__"' in contents or "mcp.run" in contents
                )

        return config

    @classmethod
    def check_deps(cls, path: Path, *deps: str) -> Set[str]:
        pending_deps = {dep.lower() for dep in deps}
        initial_deps = set(pending_deps)
        for file in discover_python_dependency_files(path):
            if file.name == "uv.lock":
                continue
            for line in file.read_text().splitlines():
                for dep in set(pending_deps):
                    if dep in line.lower():
                        pending_deps.remove(dep)
                        if not pending_deps:
                            break
                if not pending_deps:
                    break
            if not pending_deps:
                break
        return initial_deps - pending_deps

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

def format_app_import(asgi_application: str) -> str:
    # Transform "mysite.asgi.application" to "mysite.asgi:application" using regex
    return re.sub(r"\.([^.]+)$", r":\1", asgi_application)


def file_to_python_path(path: Optional[str]) -> Optional[str]:
    if not path:
        return None
    file = path.rstrip(".py").replace("/", ".").replace("\\", ".")
    return f"{file}:app"
