from shipit.providers.go import GoConfig
from shipit.providers.php import PhpProvider
from shipit.providers.php import PhpConfig


def test_php_config_app_yaml_for_php_cli() -> None:
    config = PhpConfig(phpix=False)
    assert config.app_yaml(config) == {
        "scaling": {"mode": "single_concurrency"}
    }


def test_php_config_app_yaml_for_phpix() -> None:
    config = PhpConfig(phpix=True)
    assert config.app_yaml(config) == {"memory": {"limit": "2G"}}


def test_go_config_app_yaml() -> None:
    config = GoConfig()
    assert config.app_yaml(config) == {
        "scaling": {"mode": "single_concurrency"}
    }


def test_phpix_start_command_uses_threads_flag(tmp_path) -> None:
    (tmp_path / "index.php").write_text("<?php phpinfo();")
    provider = PhpProvider(tmp_path, PhpConfig(phpix=True))
    assert "--php-threads=4 -S localhost:{}" in provider.commands()["start"]
