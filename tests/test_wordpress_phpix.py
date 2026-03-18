from pathlib import Path

import yaml

from shipit.generator import generate_shipit, load_provider_config
from shipit.providers.base import Config
from shipit.providers.wordpress import WordPressProvider
from shipit.runners.wasmer import WasmerRunner
from shipit.shipit_types import Mount, Package, Serve, Service


class DummyBuildBackend:
    def __init__(self, root: Path) -> None:
        self.root = root

    def build(self, name, env, mounts, steps) -> None:
        raise NotImplementedError

    def get_build_mount_path(self, name: str) -> Path:
        return self.root / "build" / name

    def get_artifact_mount_path(self, name: str) -> Path:
        path = self.root / "artifacts" / name
        path.mkdir(parents=True, exist_ok=True)
        return path

    def get_runtime_path(self) -> str | None:
        return None


def test_generate_shipit_wordpress_phpix_mode() -> None:
    repo_root = Path(__file__).resolve().parents[1]
    example_dir = repo_root / "examples" / "php-wordpress"

    base_config = Config()
    base_config.commands.enrich_from_path(example_dir)
    provider_config = load_provider_config(
        WordPressProvider,
        example_dir,
        base_config,
        {"phpix": True},
    )
    provider = WordPressProvider(example_dir, provider_config)

    generated = generate_shipit(example_dir, provider)

    assert "phpix = dep(" in generated
    assert 'copy("wordpress/start.php"' in generated
    assert (
        '"start": "phpix --startup-script={}/start-wp.php -S localhost:{} '
        '-t {}".format(assets.serve_path, PORT, app.serve_path)'
    ) in generated


def test_wasmer_app_yaml_sets_memory_limit_for_wordpress_phpix(
    tmp_path: Path,
) -> None:
    src_dir = tmp_path / "src"
    src_dir.mkdir()

    runner = WasmerRunner(DummyBuildBackend(tmp_path), src_dir)
    serve = Serve(
        name="wordpress",
        provider="wordpress",
        build=[],
        deps=[Package("phpix"), Package("bash")],
        commands={
            "start": "phpix -S localhost:8080 -t /app",
            "after_deploy": "bash /opt/assets/setup-wp.sh",
        },
        cwd="/app",
        mounts=[
            Mount("app", src_dir, Path("/app")),
            Mount("assets", src_dir, Path("/opt/assets")),
        ],
        env={"PHPIX_PHP_THREADS": "4"},
        services=[Service(name="database", provider="mysql")],
    )

    runner.build_serve(serve)

    app_yaml = yaml.safe_load((runner.wasmer_dir_path / "app.yaml").read_text())

    assert app_yaml["capabilities"]["database"]["engine"] == "mysql"
    assert app_yaml["capabilities"]["memory"]["limit"] == "2Gb"
    assert app_yaml["env"]["PHPIX_PHP_THREADS"] == "4"
