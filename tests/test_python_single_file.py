from __future__ import annotations

import textwrap
from pathlib import Path

from shipit.providers.base import CustomCommands
from shipit.providers.python import PythonProvider


def test_detects_single_file_python_app(tmp_path: Path) -> None:
    project_dir = tmp_path / "project"
    project_dir.mkdir()

    script = project_dir / "hello.py"
    script.write_text("print('hi')\n")

    result = PythonProvider.detect(project_dir, CustomCommands())

    assert result is not None
    assert result.name == "python"


def test_single_file_uv_script_dependencies_extracted(tmp_path: Path) -> None:
    project_dir = tmp_path / "project"
    project_dir.mkdir()

    script = project_dir / "hello.py"
    script.write_text(
        """# /// script
# dependencies = [
#   \"requests<3\",
#   \"rich\",
# ]
# python = \"3.11\"
# ///

print(\"hello\")
"""
    )

    provider = PythonProvider(project_dir, CustomCommands())

    assert provider.is_single_file_app is True
    assert provider.main_file == "app.py"
    assert not script.exists()

    relocated = project_dir / "app.py"
    assert relocated.exists()

    pyproject_path = project_dir / "pyproject.toml"
    assert pyproject_path.exists()
    pyproject_text = pyproject_path.read_text()
    expected_pyproject = textwrap.dedent(
        """
        [project]
        name = "project"
        version = "0.1.0"
        requires-python = ">=3.11"
        dependencies = [
            "requests<3",
            "rich",
        ]
        """
    ).lstrip("\n")
    if not expected_pyproject.endswith("\n"):
        expected_pyproject += "\n"

    assert pyproject_text == expected_pyproject
    assert provider.default_python_version == "3.11"
