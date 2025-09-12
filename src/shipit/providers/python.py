from __future__ import annotations

from pathlib import Path
from typing import Dict, Optional

from .base import (
    DetectResult,
    DependencySpec,
    Provider,
    _exists,
)


class PythonProvider:
    def name(self) -> str:
        return "python"

    def detect(self, path: Path) -> Optional[DetectResult]:
        if _exists(path, "pyproject.toml", "requirements.txt"):
            if _exists(path, "manage.py"):
                return DetectResult(self.name(), 70)
            return DetectResult(self.name(), 50)
        return None

    def initialize(self, path: Path) -> None:
        pass

    def serve_name(self, path: Path) -> str:
        return path.name

    def provider_kind(self, path: Path) -> str:
        return "python"

    def dependencies(self, path: Path) -> list[DependencySpec]:
        if _exists(path, ".python-version"):
            python_version = (path / ".python-version").read_text().strip()
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

    def declarations(self, path: Path) -> Optional[str]:
        return (
            "cross_platform = getenv(\"SHIPIT_PYTHON_CROSS_PLATFORM\")\n"
            "python_extra_index_url = getenv(\"SHIPIT_PYTHON_EXTRA_INDEX_URL\")\n"
            "python_cross_packages_serve_path = None\n"
            "python_cross_packages_path = None\n"
            "if cross_platform:\n"
            "  python_cross_packages_path = serve_mount(\"python-cross-packages\")\n"
            "  python_cross_packages_serve_path = f\"/.venv/lib/python{python_version}/site-packages\"\n"
            "precompile_python = getenv(\"SHIPIT_PYTHON_PRECOMPILE\") in [\"true\", \"True\", \"TRUE\", \"1\", \"on\", \"yes\", \"y\", \"Y\", \"YES\", \"On\", \"ON\"]\n"
        )

    def build_steps(self, path: Path) -> list[str]:
        return [
            "run(f\"uv sync --compile --python python{python_version} --locked --no-managed-python\", inputs=[\"pyproject.toml\", \"uv.lock\"], outputs=[\".\"], group=\"install\")",
            "run(f\"uv pip compile pyproject.toml --python-version={python_version} --universal --extra-index-url {python_extra_index_url} --index-url=https://pypi.org/simple --emit-index-url --only-binary :all: -o cross-requirements.txt\", inputs=[\"pyproject.toml\"], outputs=[\"cross-requirements.txt\"]) if cross_platform else None",
            "run(f\"uvx pip install -r cross-requirements.txt --target {python_cross_packages_path} --platform {cross_platform} --only-binary=:all: --python-version={python_version} --compile\", outputs=[\".\"]) if cross_platform else None",
            "run(\"rm cross-requirements.txt\") if cross_platform else None",
            "path(\".venv/bin\")",
            "copy(\".\", \".\", ignore=[\".venv\", \".git\", \"__pycache__\"])",
            "run(\"rm -rf .venv\") if cross_platform else None",
        ]

    def prepare_steps(self, path: Path) -> Optional[list[str]]:
        return [
            'run("echo \\\"Precompiling Python code...\\\"") if precompile_python and python_cross_packages_serve_path else None',
            'run(f"python -m compileall -o 2 {python_cross_packages_serve_path}") if precompile_python and python_cross_packages_serve_path else None',
            'run("echo \\\"Precompiling package code...\\\"") if precompile_python else None',
            'run("python -m compileall -o 2 .") if precompile_python else None',
        ]

    def commands(self, path: Path) -> Dict[str, str]:
        if _exists(path, "manage.py"):
            start_cmd = '"python manage.py runserver 0.0.0.0:8000"'
            migrate_cmd = '"python manage.py migrate"'
            return {"start": start_cmd, "after_deploy": migrate_cmd}
        elif _exists(path, "main.py"):
            start_cmd = '"python main.py"'
        elif _exists(path, "src/main.py"):
            start_cmd = '"python src/main.py"'
        else:
            start_cmd = '"python -c \'print(\\\"Hello, World!\\\")\'"'
        return {"start": start_cmd}

    def assets(self, path: Path) -> Optional[Dict[str, str]]:
        return None

    def mounts(self, path: Path) -> Optional[Dict[str, str]]:
        return {"python_cross_packages_serve_path": "python_cross_packages_path"}
