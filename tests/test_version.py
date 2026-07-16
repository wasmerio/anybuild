import sys
from pathlib import Path

import pytest
import tomlkit

from shipit import cli
from shipit.version import version as module_version


def test_module_version_matches_pyproject() -> None:
    pyproject_path = Path(__file__).resolve().parents[1] / "pyproject.toml"
    data = tomlkit.parse(pyproject_path.read_text())
    project_version = data["project"]["version"]
    assert module_version == project_version, (
        f"module version {module_version} != pyproject version {project_version}"
    )


@pytest.mark.parametrize("option", ["--version", "-v"])
def test_cli_version_is_plain_text(option: str, monkeypatch, capsys) -> None:
    monkeypatch.setattr(sys, "argv", ["shipit", option])

    with pytest.raises(SystemExit) as exc_info:
        cli.main()

    assert exc_info.value.code == 0
    assert capsys.readouterr().out == f"{module_version}\n"
