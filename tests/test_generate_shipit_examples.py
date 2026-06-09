from pathlib import Path

import pytest

from shipit.cli import (
    ProjectPaths,
    apply_subdir_provider_config,
    apply_subdir_workspace_config,
)
from shipit.generator import generate_shipit, load_provider, load_provider_config
from shipit.providers.base import Config


SUBDIR_EXAMPLES = {
    "node-npm-file-subdir": "apps/dashboard",
    "node-pnpm-workspace-subdir": "apps/dashboard",
}


def _example_dirs_with_shipit() -> list[Path]:
    root = Path(__file__).resolve().parent.parent
    examples = root / "examples"
    return [p for p in examples.iterdir() if (p / "Shipit").is_file()]


_EXAMPLE_DIRS = _example_dirs_with_shipit()


@pytest.mark.parametrize(
    "example_dir", _EXAMPLE_DIRS, ids=[p.name for p in _EXAMPLE_DIRS]
)
def test_generate_shipit_matches_example(
    example_dir: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Ensure generated Shipit content matches the checked-in file.

    This validates provider detection and the Shipit generator formatting for
    each example that includes a `Shipit` file.
    """
    if example_dir.name == "php-wordpress-empty":
        monkeypatch.setenv("SHIPIT_WP_VERSION", "latest")
        monkeypatch.setenv("SHIPIT_PHPIX", "true")

    subdir = SUBDIR_EXAMPLES.get(example_dir.name)
    app_path = example_dir / subdir if subdir else example_dir
    base_config = Config()
    base_config.commands.enrich_from_path(app_path)

    provider_cls = load_provider(app_path, base_config)
    provider_config = load_provider_config(provider_cls, app_path, base_config)
    project_paths = ProjectPaths(example_dir, app_path, subdir)
    apply_subdir_provider_config(project_paths, provider_config)
    apply_subdir_workspace_config(project_paths, provider_config)
    provider = provider_cls(app_path, provider_config)

    generated = generate_shipit(app_path, provider, subdir=subdir)
    expected = (example_dir / "Shipit").read_text()
    # Use raw assert to let pytest show a unified diff on mismatch
    assert generated == expected
