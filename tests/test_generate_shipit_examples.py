from __future__ import annotations

from pathlib import Path

import pytest

from shipit.generator import generate_shipit


def _example_dirs_with_shipit() -> list[Path]:
    root = Path(__file__).resolve().parent.parent
    examples = root / "examples"
    return [p for p in examples.iterdir() if (p / "Shipit").is_file()]


_EXAMPLE_DIRS = _example_dirs_with_shipit()


@pytest.mark.parametrize(
    "example_dir", _EXAMPLE_DIRS, ids=[p.name for p in _EXAMPLE_DIRS]
)
def test_generate_shipit_matches_example(example_dir: Path) -> None:
    """Ensure generated Shipit content matches the checked-in file.

    This validates provider detection and the Shipit generator formatting for
    each example that includes a `Shipit` file.
    """
    generated = generate_shipit(example_dir)
    expected = (example_dir / "Shipit").read_text()
    # Use raw assert to let pytest show a unified diff on mismatch
    assert generated == expected
