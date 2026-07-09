"""Unit tests for the pure detection helpers behind PythonProvider.load_config."""

from pathlib import Path

from shipit.providers.python import (
    DatabaseType,
    MigrationStrategy,
    PythonFramework,
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
