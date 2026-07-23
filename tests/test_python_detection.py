"""Unit tests for the pure detection helpers behind PythonProvider.load_config."""

from pathlib import Path

import pytest

from shipit.providers.base import Config
from shipit.providers.python import (
    DatabaseType,
    MigrationStrategy,
    PythonConfig,
    PythonFramework,
    PythonProvider,
    PythonServer,
    _default_server_for_framework,
    _detect_database,
    _detect_framework,
    _detect_mcp_self_running,
    _detect_migration_strategy,
    _detect_server,
    _resolve_applications,
)


def test_detect_server_precedence() -> None:
    assert _detect_server({"uvicorn", "daphne"}) == PythonServer.Uvicorn
    assert _detect_server({"hypercorn", "daphne"}) == PythonServer.Hypercorn
    assert _detect_server({"daphne"}) == PythonServer.Daphne
    assert _detect_server({"flask"}) is None


def test_detect_framework_precedence(tmp_path: Path) -> None:
    # Django requires manage.py in addition to the dependency.
    assert _detect_framework(tmp_path, {"django", "fastapi"}) == PythonFramework.FastAPI
    (tmp_path / "manage.py").write_text("")
    assert _detect_framework(tmp_path, {"django", "fastapi"}) == PythonFramework.Django
    # Streamlit beats MCP beats FastAPI beats Flask beats FastHTML.
    assert _detect_framework(tmp_path, {"streamlit", "mcp"}) == PythonFramework.Streamlit
    assert _detect_framework(tmp_path, {"mcp", "fastapi"}) == PythonFramework.MCP
    assert _detect_framework(tmp_path, {"flask", "python-fasthtml"}) == PythonFramework.Flask
    assert _detect_framework(tmp_path, set()) is None


def test_detect_migration_strategy(tmp_path: Path) -> None:
    assert (
        _detect_migration_strategy(tmp_path, PythonFramework.Django, set())
        == MigrationStrategy.Django
    )
    assert (
        _detect_migration_strategy(tmp_path, None, {"alembic"})
        == MigrationStrategy.Alembic
    )
    (tmp_path / "alembic.ini").write_text("")
    assert (
        _detect_migration_strategy(tmp_path, None, set()) == MigrationStrategy.Alembic
    )


def test_default_server_for_framework() -> None:
    assert _default_server_for_framework(PythonFramework.Django) == PythonServer.Uvicorn
    assert _default_server_for_framework(PythonFramework.FastHTML) == PythonServer.Uvicorn
    assert _default_server_for_framework(PythonFramework.Streamlit) is None
    assert _default_server_for_framework(None) is None


def test_resolve_applications_django_settings(tmp_path: Path) -> None:
    settings = tmp_path / "mysite" / "settings.py"
    settings.parent.mkdir()
    settings.write_text('WSGI_APPLICATION = "mysite.wsgi.application"\n')

    asgi, wsgi = _resolve_applications(tmp_path, PythonFramework.Django, None)
    assert asgi is None
    assert wsgi == "mysite.wsgi:application"

    settings.write_text('ASGI_APPLICATION = "mysite.asgi.application"\n')
    asgi, wsgi = _resolve_applications(tmp_path, PythonFramework.Django, None)
    assert asgi == "mysite.asgi:application"
    assert wsgi is None


def test_resolve_applications_from_main_file(tmp_path: Path) -> None:
    asgi, wsgi = _resolve_applications(tmp_path, PythonFramework.FastAPI, "src/main.py")
    assert (asgi, wsgi) == ("src.main:app", None)
    asgi, wsgi = _resolve_applications(tmp_path, PythonFramework.Flask, "main.py")
    assert (asgi, wsgi) == (None, "main:app")


def test_detect_database_mysql_wins() -> None:
    assert _detect_database({"pymysql", "psycopg"}) == DatabaseType.MySQL
    assert _detect_database({"asyncpg"}) == DatabaseType.PostgreSQL
    assert _detect_database({"flask"}) is None


def test_detect_mcp_self_running(tmp_path: Path) -> None:
    (tmp_path / "main.py").write_text("mcp.run()\n")
    assert _detect_mcp_self_running(tmp_path, PythonFramework.MCP, "main.py") is True
    (tmp_path / "main.py").write_text("app = build_server()\n")
    assert _detect_mcp_self_running(tmp_path, PythonFramework.MCP, "main.py") is False
    # Only applies to MCP projects with a resolvable main file.
    assert _detect_mcp_self_running(tmp_path, PythonFramework.FastAPI, "main.py") is False
    assert _detect_mcp_self_running(tmp_path, PythonFramework.MCP, None) is False


def test_python_provider_warns_when_start_command_is_missing(
    tmp_path: Path,
    capsys,
) -> None:
    (tmp_path / "pyproject.toml").write_text("[project]\nname = 'app'\n")

    provider_config = PythonProvider.load_config(tmp_path, Config())

    assert provider_config.commands.start is None
    assert (
        "Warning: no start command could be inferred for Python project"
        in capsys.readouterr().err
    )


def test_python_provider_does_not_warn_when_start_command_is_inferred(
    tmp_path: Path,
    capsys,
) -> None:
    (tmp_path / "pyproject.toml").write_text("[project]\nname = 'app'\n")
    (tmp_path / "main.py").write_text("print('hello')\n")

    provider_config = PythonProvider.load_config(tmp_path, Config())

    assert provider_config.commands.start == "python main.py"
    assert "no start command could be inferred" not in capsys.readouterr().err


@pytest.mark.parametrize(
    ("config", "expected"),
    [
        (
            PythonConfig(
                server=PythonServer.Daphne,
                asgi_application="app:api",
            ),
            "daphne app:api --bind 0.0.0.0 --port $PORT",
        ),
        (
            PythonConfig(
                server=PythonServer.Uvicorn,
                asgi_application="app:api",
            ),
            "uvicorn app:api --host 0.0.0.0 --port $PORT",
        ),
        (
            PythonConfig(
                server=PythonServer.Uvicorn,
                wsgi_application="app:web",
            ),
            (
                "uvicorn app:web --interface=wsgi "
                "--host 0.0.0.0 --port $PORT"
            ),
        ),
        (
            PythonConfig(
                server=PythonServer.Hypercorn,
                asgi_application="app:api",
            ),
            "hypercorn app:api --bind 0.0.0.0:$PORT",
        ),
        (
            PythonConfig(
                framework=PythonFramework.Streamlit,
                main_file="streamlit_app.py",
            ),
            (
                "streamlit run streamlit_app.py --server.port $PORT "
                "--server.address 0.0.0.0 --server.headless true"
            ),
        ),
        (
            PythonConfig(
                framework=PythonFramework.MCP,
                main_file="main.py",
            ),
            (
                "python $VIRTUAL_ENV/bin/mcp run main.py "
                "--transport=streamable-http"
            ),
        ),
        (
            PythonConfig(framework=PythonFramework.Django),
            "python manage.py runserver 0.0.0.0:$PORT",
        ),
        (
            PythonConfig(main_file="main.py"),
            "python main.py",
        ),
    ],
)
def test_infer_start_command(
    config: PythonConfig,
    expected: str,
) -> None:
    assert PythonProvider.infer_start_command(config) == expected
