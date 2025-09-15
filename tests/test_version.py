from pathlib import Path

import tomlkit

from shipit.version import version as module_version


def test_module_version_matches_pyproject() -> None:
    pyproject_path = Path(__file__).resolve().parents[1] / "pyproject.toml"
    data = tomlkit.parse(pyproject_path.read_text())
    project_version = data["project"]["version"]
    assert (
        module_version == project_version
    ), f"module version {module_version} != pyproject version {project_version}"

