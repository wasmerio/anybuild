from shipit.providers.base import Config
from shipit.providers.php import PhpProvider
import pytest


def test_php_provider_defaults_to_php_binary(tmp_path) -> None:
    (tmp_path / "index.php").write_text("<?php echo 'ok';")

    config = PhpProvider.load_config(tmp_path, Config())
    provider = PhpProvider(tmp_path, config)

    assert config.phpix is False
    assert provider.dependencies()[0].name == "php"
    assert provider.commands()["start"].startswith('"php -S 127.0.0.1:')


@pytest.mark.parametrize("value", ["true", "1"])
def test_php_provider_uses_phpix_dependency_when_env_enabled(
    tmp_path, monkeypatch, value: str
) -> None:
    (tmp_path / "index.php").write_text("<?php echo 'ok';")
    monkeypatch.setenv("SHIPIT_PHPIX", value)

    config = PhpProvider.load_config(tmp_path, Config())
    provider = PhpProvider(tmp_path, config)

    assert config.phpix is True
    assert provider.dependencies()[0].name == "phpix"
    assert provider.commands()["start"].startswith('"php -S 127.0.0.1:')
