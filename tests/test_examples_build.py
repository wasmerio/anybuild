from pathlib import Path
from typing import List

import pytest
from typer.testing import CliRunner

import shutil

from shipit.cli import app, LocalBuilder


def list_example_dirs() -> List[Path]:
    root = Path(__file__).resolve().parents[1]
    examples_dir = root / "examples"
    result: List[Path] = []
    if not examples_dir.exists():
        return result
    for path in sorted(examples_dir.iterdir()):
        if path.is_dir() and (path / "Shipit").exists():
            result.append(path)
    return result


@pytest.mark.parametrize("example_dir", list_example_dirs(), ids=lambda p: p.name)
def test_shipit_build_examples_noop_commands(monkeypatch: pytest.MonkeyPatch, example_dir: Path) -> None:
    # Make all external commands no-ops to avoid network and toolchain deps.
    # 1) Pretend every required program exists by resolving to a benign executable.
    true_exe = shutil.which("true") or "/usr/bin/true"
    monkeypatch.setattr(shutil, "which", lambda *_args, **_kwargs: true_exe)
    # 2) Neutralize any builder-level command invocations (e.g., wasmer during prepare).
    monkeypatch.setattr(LocalBuilder, "run_command", lambda self, command, extra_args=None: None, raising=True)

    runner = CliRunner()
    # Use --wasmer to ensure examples that rely on cross-platform env build.
    result = runner.invoke(app, ["build", "--wasmer", str(example_dir)])

    # Basic sanity: command runs successfully
    assert result.exit_code == 0, result.output

    # Stable output lines from Shipit during a successful build
    out = result.output
    # Stable, provider-agnostic output
    assert "Shipit" in out
    assert "Building package" in out
    assert "Build complete" in out
