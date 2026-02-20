from pathlib import Path

import pytest

from shipit.generator import generate_shipit, load_provider, load_provider_config
from shipit.providers.base import Config


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
    base_config = Config()
    base_config.commands.enrich_from_path(example_dir)

    provider_cls = load_provider(example_dir, base_config)
    provider_config = load_provider_config(provider_cls, example_dir, base_config)
    provider = provider_cls(example_dir, provider_config)

    generated = generate_shipit(example_dir, provider)
    expected = (example_dir / "Shipit").read_text()
    # Use raw assert to let pytest show a unified diff on mismatch
    assert generated == expected
