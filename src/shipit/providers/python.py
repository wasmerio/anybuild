from enum import Enum
import re
from pathlib import Path
from typing import List, Optional, Set

from pydantic import Field
from pydantic_settings import SettingsConfigDict

from shipit.ui import console

from .install_context import discover_python_dependency_files, discover_python_install_context
from .base import DetectResult, _exists, Config


class PythonFramework(str, Enum):
    Django = "django"
    Streamlit = "streamlit"
    FastAPI = "fastapi"
    Flask = "flask"
    FastHTML = "python-fasthtml"
    MCP = "mcp"


class PythonServer(str, Enum):
    Hypercorn = "hypercorn"
    Uvicorn = "uvicorn"
    # Gunicorn = "gunicorn"
    Daphne = "daphne"


class MigrationStrategy(str, Enum):
    Django = "django"
    Alembic = "alembic"



class DatabaseType(str, Enum):
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

        must_have_deps = must_have_deps or set()
        found_deps = cls.check_deps(
            path,
            *DEPENDENCY_SCAN,
            *MYSQL_DEPS,
            *PG_DEPS,
            *must_have_deps,
        )

        if _requires_all_files(path, found_deps):
            config.install_requires_all_files = True

        if not config.server:
            config.server = _detect_server(found_deps)

        if "ffmpeg" in found_deps:
            config.uses_ffmpeg = True
        if "pandoc" in found_deps:
            config.uses_pandoc = True

        if not config.framework:
            config.framework = _detect_framework(path, found_deps)
            if config.framework == PythonFramework.Django:
                config.migration_strategy = MigrationStrategy.Django

        if not config.migration_strategy:
            config.migration_strategy = _detect_migration_strategy(
                path, config.framework, found_deps
            )

        if not config.server and config.framework:
            config.server = _default_server_for_framework(config.framework)
            if config.server == PythonServer.Uvicorn:
                must_have_deps.add("uvicorn")

        if not config.asgi_application and not config.wsgi_application:
            asgi, wsgi = _resolve_applications(path, config.framework, config.main_file)
            config.asgi_application = asgi
            config.wsgi_application = wsgi

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
            config.database = _detect_database(found_deps)

        config.install_inputs = _compute_install_inputs(path)
        config.mcp_self_running = _detect_mcp_self_running(
            path, config.framework, config.main_file
        )

        if not config.commands.start:
            config.commands.start = cls.infer_start_command(config)
        if not config.commands.start:
            console.print(
                "[bold yellow]Warning:[/bold yellow] "
                "no start command could be inferred for Python project"
            )

        return config

    @staticmethod
    def infer_start_command(config: PythonConfig) -> Optional[str]:
        main_file = config.main_file
        asgi = config.asgi_application
        wsgi = config.wsgi_application

        if config.server == PythonServer.Daphne:
            if asgi:
                return f"daphne {asgi} --bind 0.0.0.0 --port $PORT"
            return None
        if config.server == PythonServer.Uvicorn:
            if asgi:
                return f"uvicorn {asgi} --host 0.0.0.0 --port $PORT"
            if wsgi:
                return (
                    f"uvicorn {wsgi} --interface=wsgi "
                    "--host 0.0.0.0 --port $PORT"
                )
            if not main_file:
                return None
        elif config.server == PythonServer.Hypercorn:
            if asgi:
                return f"hypercorn {asgi} --bind 0.0.0.0:$PORT"
            return None
        elif config.framework == PythonFramework.Streamlit:
            if main_file:
                return (
                    f"streamlit run {main_file} --server.port $PORT "
                    "--server.address 0.0.0.0 --server.headless true"
                )
            return None
        elif config.framework == PythonFramework.MCP:
            if not main_file:
                return None
            if config.mcp_self_running:
                return f"python {main_file}"
            return (
                f"python $VIRTUAL_ENV/bin/mcp run {main_file} "
                "--transport=streamable-http"
            )
        elif config.framework == PythonFramework.Django:
            return "python manage.py runserver 0.0.0.0:$PORT"

        if main_file:
            return f"python {main_file}"
        return None

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


PG_DEPS = {
    "asyncpg",
    "aiopg",
    "psycopg",
    "psycopg2",
    "psycopg-binary",
    "psycopg2-binary",
}

MYSQL_DEPS = {
    "mysqlclient",
    "pymysql",
    "mysql-connector-python",
    "aiomysql",
    "asyncmy",
    "mariadb",
}

# Dependencies scanned to derive framework/server/tooling decisions.
DEPENDENCY_SCAN = (
    "file://",  # Not a dependency: signals that installs need all files.
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
    "ffmpeg",
    "pandoc",
)


def _requires_all_files(path: Path, found_deps: Set[str]) -> bool:
    """Installs that reference local files need the whole tree staged."""
    if "file://" in found_deps:
        return True
    return discover_python_install_context(
        path,
        include_pyproject=_exists(path, "pyproject.toml"),
        include_requirements=_exists(path, "requirements.txt"),
    ).requires_all_files


def _detect_server(found_deps: Set[str]) -> Optional[PythonServer]:
    if "uvicorn" in found_deps:
        return PythonServer.Uvicorn
    if "hypercorn" in found_deps:
        return PythonServer.Hypercorn
    if "daphne" in found_deps:
        return PythonServer.Daphne
    return None


def _detect_framework(path: Path, found_deps: Set[str]) -> Optional[PythonFramework]:
    if _exists(path, "manage.py") and "django" in found_deps:
        return PythonFramework.Django
    if "streamlit" in found_deps:
        return PythonFramework.Streamlit
    if "mcp" in found_deps:
        return PythonFramework.MCP
    if "fastapi" in found_deps:
        return PythonFramework.FastAPI
    if "flask" in found_deps:
        return PythonFramework.Flask
    if "python-fasthtml" in found_deps:
        return PythonFramework.FastHTML
    return None


def _detect_migration_strategy(
    path: Path,
    framework: Optional[PythonFramework],
    found_deps: Set[str],
) -> Optional[MigrationStrategy]:
    if framework == PythonFramework.Django:
        return MigrationStrategy.Django
    if "alembic" in found_deps or _exists(path, "alembic.ini"):
        return MigrationStrategy.Alembic
    return None


def _default_server_for_framework(
    framework: Optional[PythonFramework],
) -> Optional[PythonServer]:
    if framework in (
        PythonFramework.Django,
        PythonFramework.FastAPI,
        PythonFramework.Flask,
        PythonFramework.FastHTML,
    ):
        return PythonServer.Uvicorn
    return None


def _resolve_applications(
    path: Path,
    framework: Optional[PythonFramework],
    main_file: Optional[str],
) -> tuple[Optional[str], Optional[str]]:
    """Locate the (asgi, wsgi) application imports for the framework."""
    asgi: Optional[str] = None
    wsgi: Optional[str] = None
    if framework == PythonFramework.Django:
        settings_file = next(path.glob("**/settings.py"), None)
        if settings_file:
            settings = settings_file.read_text()
            asgi_match = re.search(r"ASGI_APPLICATION\s*=\s*['\"](.*)['\"]", settings)
            if asgi_match:
                asgi = format_app_import(asgi_match.group(1))
            else:
                wsgi_match = re.search(
                    r"WSGI_APPLICATION\s*=\s*['\"](.*)['\"]", settings
                )
                if wsgi_match:
                    wsgi = format_app_import(wsgi_match.group(1))

    python_path = file_to_python_path(main_file)
    if framework == PythonFramework.FastAPI:
        asgi = python_path
    elif framework == PythonFramework.Flask:
        wsgi = python_path
    elif framework == PythonFramework.MCP:
        asgi = python_path
    elif framework == PythonFramework.FastHTML:
        asgi = python_path
    return asgi, wsgi


def _detect_database(found_deps: Set[str]) -> Optional[DatabaseType]:
    if MYSQL_DEPS & found_deps:
        return DatabaseType.MySQL
    if PG_DEPS & found_deps:
        return DatabaseType.PostgreSQL
    return None


def _compute_install_inputs(path: Path) -> Optional[List[str]]:
    if _exists(path, "pyproject.toml"):
        return discover_python_install_context(path, include_pyproject=True).inputs
    return discover_python_install_context(
        path, include_requirements=_exists(path, "requirements.txt")
    ).inputs


def _detect_mcp_self_running(
    path: Path,
    framework: Optional[PythonFramework],
    main_file: Optional[str],
) -> bool:
    """Whether the MCP main file starts its own server."""
    if framework != PythonFramework.MCP or not main_file:
        return False
    main_path = path / main_file
    if not main_path.is_file():
        return False
    contents = main_path.read_text()
    return 'if __name__ == "__main__"' in contents or "mcp.run" in contents
