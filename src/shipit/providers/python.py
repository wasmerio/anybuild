from __future__ import annotations

from pathlib import Path
from typing import Dict, Optional

from .base import (
    DetectResult,
    DependencySpec,
    Provider,
    _exists,
    MountSpec,
)


class PythonProvider:
    def __init__(self, path: Path):
        self.path = path
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
        if _exists(self.path, ".python-version"):
            python_version = (self.path / ".python-version").read_text().strip()
        else:
            python_version = "3.13"

        return [
            DependencySpec(
                "python",
                env_var="SHIPIT_PYTHON_VERSION",
                default_version=python_version,
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
            "python_cross_packages_path = venv[\"build\"] + f\"/lib/python{python_version}/site-packages\""
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
            steps += [
                "env(UV_PROJECT_ENVIRONMENT=local_venv[\"build\"] if cross_platform else venv[\"build\"])",
                "run(f\"uv sync --compile --python python{python_version} --no-managed-python" + extra_args + "\", inputs=[" + inputs + "], group=\"install\")",
                "run(f\"uv pip compile pyproject.toml --python-version={python_version} --universal --extra-index-url {python_extra_index_url} --index-url=https://pypi.org/simple --emit-index-url --only-binary :all: -o cross-requirements.txt\", inputs=[\"pyproject.toml\"], outputs=[\"cross-requirements.txt\"]) if cross_platform else None",
                "run(f\"uvx pip install -r cross-requirements.txt --target {python_cross_packages_path} --platform {cross_platform} --only-binary=:all: --python-version={python_version} --compile\") if cross_platform else None",
                "run(\"rm cross-requirements.txt\") if cross_platform else None",
            ]
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
            'workdir(app["serve"])',
            'run("echo \\\"Precompiling Python code...\\\"") if precompile_python else None',
            'run("python -m compileall -o 2 $PYTHONPATH") if precompile_python else None',
            'run("echo \\\"Precompiling package code...\\\"") if precompile_python else None',
            'run("python -m compileall -o 2 .") if precompile_python else None',
        ]

    def commands(self) -> Dict[str, str]:
        if _exists(self.path, "manage.py"):
            start_cmd = '"python manage.py runserver 0.0.0.0:8000"'
            migrate_cmd = '"python manage.py migrate"'
            return {"start": start_cmd, "after_deploy": migrate_cmd}
        elif _exists(self.path, "main.py"):
            start_cmd = '"python main.py"'
        elif _exists(self.path, "src/main.py"):
            start_cmd = '"python src/main.py"'
        else:
            start_cmd = '"python -c \'print(\\\"Hello, World!\\\")\'"'
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
            "PYTHONPATH": "\"{}/lib/python{}/site-packages\".format(venv[\"serve\"], python_version)"
        }
