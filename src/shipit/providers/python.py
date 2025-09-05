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
        if _exists(path, "pyproject.toml"):
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

    def build_dependencies(self, path: Path) -> list[DependencySpec]:
        return [
            DependencySpec("python", env_var="SHIPIT_PYTHON_VERSION", default_version="3.13"),
            DependencySpec("uv", env_var="SHIPIT_UV_VERSION", default_version="0.8.15"),
        ]

    def serve_dependencies(self, path: Path) -> list[DependencySpec]:
        return [DependencySpec("python", env_var="SHIPIT_PYTHON_VERSION", default_version="3.13")]

    def declarations(self, path: Path) -> Optional[str]:
        return (
            "cross_platform = getenv(\"SHIPIT_PYTHON_CROSS_PLATFORM\")\n"
            "python_extra_index_url = getenv(\"SHIPIT_PYTHON_EXTRA_INDEX_URL\")\n"
            "python_cross_packages_serve_path = \"\"\n"
            "python_cross_packages_path = None\n"
            "if cross_platform:\n"
            "  python_cross_packages_path = serve_mount(\"python-cross-packages\")\n"
            "  if cross_platform == \"wasix_wasm32\":\n"
            "    python_cross_packages_serve_path = f\"/cpython/lib/python{python_version}/site-packages\"\n"
        )

    def build_steps(self, path: Path) -> list[str]:
        return [
            "use(python, uv)",
            "run(\"uv sync --compile --no-managed-python\", inputs=[\"pyproject.toml\", \"uv.lock\", \".python-version\"], outputs=[\".\"], group=\"install\")",
            "run(f\"uv pip compile pyproject.toml --python-version={python_version} --universal --extra-index-url {python_extra_index_url} --index-url=https://pypi.org/simple --emit-index-url --only-binary :all: -o cross-requirements.txt\") if cross_platform else None",
            "run(f\"uvx pip install -r cross-requirements.txt --target {python_cross_packages_path} --platform {cross_platform} --only-binary=:all: --python-version={python_version} --compile\") if cross_platform else None",
            "path(\".venv/bin\")",
            "copy(\".\", \".\", ignore=[\".venv\", \".git\", \"__pycache__\"])",
            "run(\"rm -rf .venv\") if cross_platform else None",
        ]

    def prepare_script(self, path: Path) -> Optional[str]:
        return (
            "echo \"Precompiling Python code...\"\n"
            "python -m compileall -o 2 {python_cross_packages_serve_path}\n"
            "echo \"Precompiling package code...\"\n"
            "python -m compileall -o 2 .\n"
        )

    def commands(self, path: Path) -> Dict[str, str]:
        if _exists(path, "manage.py"):
            start_cmd = '"python manage.py runserver 0.0.0.0:8000"'
            migrate_cmd = '"python manage.py migrate"'
            return {"start": start_cmd, "after_deploy": migrate_cmd}
        elif _exists(path, "main.py"):
            start_cmd = '"python main.py"'
        else:
            start_cmd = '"python -c \'print(\\\"Hello, World!\\\")\'"'
        return {"start": start_cmd}

    def assets(self, path: Path) -> Optional[Dict[str, str]]:
        return None

    def mounts(self, path: Path) -> Optional[Dict[str, str]]:
        return {"python_cross_packages_serve_path": "python_cross_packages_path"}

