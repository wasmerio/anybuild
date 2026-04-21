import json
from pathlib import Path

from shipit.generator import load_provider, load_provider_config
from shipit.providers.base import Config
from shipit.providers.php import PhpFramework, PhpProvider


def test_php_provider_detects_moodle_from_source(tmp_path: Path) -> None:
    project_dir = tmp_path / "moodle"
    (project_dir / "admin" / "cli").mkdir(parents=True)
    (project_dir / "lib").mkdir()
    (project_dir / "mod").mkdir()
    (project_dir / "index.php").write_text("<?php\n")
    (project_dir / "version.php").write_text("<?php\n")
    (project_dir / "lib" / "setup.php").write_text("<?php\n")
    (project_dir / "admin" / "cli" / "install.php").write_text("<?php\n")

    base_config = Config()

    provider_cls = load_provider(project_dir, base_config)
    provider_config = load_provider_config(provider_cls, project_dir, base_config)

    assert provider_cls is PhpProvider
    assert provider_config.framework == PhpFramework.Moodle


def test_php_provider_detects_drupal_from_source(tmp_path: Path) -> None:
    project_dir = tmp_path / "drupal"
    (project_dir / "web" / "core" / "lib").mkdir(parents=True)
    (project_dir / "web" / "index.php").write_text("<?php\n")
    (project_dir / "web" / "core" / "lib" / "Drupal.php").write_text(
        "<?php\n"
    )
    (project_dir / "composer.json").write_text(
        json.dumps(
            {
                "name": "drupal/recommended-project",
                "require": {"drupal/core-recommended": "^11.0"},
            }
        )
    )

    base_config = Config()

    provider_cls = load_provider(project_dir, base_config)
    provider_config = load_provider_config(provider_cls, project_dir, base_config)
    provider = provider_cls(project_dir, provider_config)

    assert provider_cls is PhpProvider
    assert provider_config.framework == PhpFramework.Drupal
    assert provider.commands()["start"] == (
        '"php -S localhost:{} -t {}/web".format(PORT, app.serve_path)'
    )


def test_php_provider_detects_drupal_from_source_layout(tmp_path: Path) -> None:
    project_dir = tmp_path / "drupal-source"
    (project_dir / "core" / "lib").mkdir(parents=True)
    (project_dir / "index.php").write_text("<?php\n")
    (project_dir / "core" / "lib" / "Drupal.php").write_text("<?php\n")

    base_config = Config()

    provider_cls = load_provider(project_dir, base_config)
    provider_config = load_provider_config(provider_cls, project_dir, base_config)

    assert provider_cls is PhpProvider
    assert provider_config.framework == PhpFramework.Drupal
