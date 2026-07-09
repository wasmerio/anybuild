from pathlib import Path

import yaml

from shipit.builders import LocalBuildBackend
from shipit.cli import evaluate_shipit
from shipit.generator import generate_shipit, load_provider, load_provider_config
from shipit.providers.base import Config
from shipit.providers.wordpress import WordPressConfig, WordPressProvider
from shipit.runners import LocalRunner
from shipit.runners.wasmer import WasmerRunner
from shipit.shipit_types import CopyStep, Mount, Package, RunStep, Serve, Service
from shipit.version import version as shipit_version


def _write_plugin(project_dir: Path, filename: str = "my-plugin.php") -> None:
    project_dir.mkdir()
    (project_dir / filename).write_text(
        """<?php
/**
 * Plugin Name: My Plugin
 */
"""
    )


def _write_theme(project_dir: Path) -> None:
    project_dir.mkdir()
    (project_dir / "style.css").write_text(
        """/*
Theme Name: My Theme
*/
"""
    )
    (project_dir / "index.php").write_text("<?php\n")


def _generate_for_path(project_dir: Path) -> tuple[type, WordPressConfig, str]:
    base_config = Config()
    base_config.commands.enrich_from_path(project_dir)
    provider_cls = load_provider(project_dir, base_config)
    provider_config = load_provider_config(provider_cls, project_dir, base_config)
    provider = provider_cls(project_dir, provider_config)
    return provider_cls, provider_config, generate_shipit(project_dir, provider)


def _evaluate_generated(
    project_dir: Path, provider_config: Config, generated: str, tmp_path: Path
):
    shipit_file = tmp_path / "Shipit.generated"
    shipit_file.write_text(generated)
    build_backend = LocalBuildBackend(
        project_dir, tmp_path / "assets", shipit_dir=tmp_path / ".shipit"
    )
    local_runner = LocalRunner(
        build_backend, project_dir, shipit_dir=tmp_path / ".shipit"
    )
    return build_backend, *evaluate_shipit(
        shipit_file,
        build_backend,
        local_runner,
        provider_config,
        project_root=project_dir,
    )


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


def test_wordpress_provider_detects_plugin_and_generates_activation(
    tmp_path: Path,
) -> None:
    project_dir = tmp_path / "my-plugin"
    _write_plugin(project_dir)

    provider_cls, provider_config, generated = _generate_for_path(project_dir)

    assert provider_cls is WordPressProvider
    assert provider_config.wp_version == "latest"

    build_backend, _ctx, serve = _evaluate_generated(
        project_dir, provider_config, generated, tmp_path
    )
    assert any(
        isinstance(step, RunStep) and "--version=latest" in step.command
        for step in serve.build
    )
    wpcontent_path = build_backend.get_build_mount_path("wpcontent_base")
    assert any(
        isinstance(step, CopyStep)
        and step.source == "."
        and step.target == f"{wpcontent_path}/plugins/my-plugin"
        and step.ignore == [".git", ".source"]
        for step in serve.build
    )
    assert serve.env
    assert serve.env["WP_PLUGINS_ACTIVATE"] == "my-plugin/my-plugin.php"


def test_wordpress_provider_detects_theme_and_generates_activation(
    tmp_path: Path,
) -> None:
    project_dir = tmp_path / "my-theme"
    _write_theme(project_dir)

    provider_cls, provider_config, generated = _generate_for_path(project_dir)

    assert provider_cls is WordPressProvider
    assert provider_config.wp_version == "latest"

    build_backend, _ctx, serve = _evaluate_generated(
        project_dir, provider_config, generated, tmp_path
    )
    assert any(
        isinstance(step, RunStep) and "--version=latest" in step.command
        for step in serve.build
    )
    wpcontent_path = build_backend.get_build_mount_path("wpcontent_base")
    assert any(
        isinstance(step, CopyStep)
        and step.source == "."
        and step.target == f"{wpcontent_path}/themes/my-theme"
        and step.ignore == [".git", ".source"]
        for step in serve.build
    )
    assert serve.env
    assert serve.env["WP_DEFAULT_THEME"] == "my-theme"


def test_wordpress_extension_keeps_user_wp_version(tmp_path: Path) -> None:
    project_dir = tmp_path / "my-plugin"
    _write_plugin(project_dir)

    base_config = Config()
    provider_config = load_provider_config(
        WordPressProvider,
        project_dir,
        base_config,
        {"wp_version": "6.8.3"},
    )

    assert provider_config.wp_version == "6.8.3"


def test_generate_shipit_wordpress_phpix_mode(tmp_path: Path) -> None:
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

    build_backend, ctx, serve = _evaluate_generated(
        example_dir, provider_config, generated, tmp_path
    )
    assert any(pkg.name == "phpix" for pkg in serve.deps)
    assert any(
        isinstance(step, CopyStep) and step.source == "wordpress/start.php"
        for step in serve.build
    )
    assets_serve = serve.mounts[1].serve_path
    app_serve = serve.mounts[0].serve_path
    assert serve.commands["start"] == (
        f"phpix --startup-script={assets_serve}/start-wp.php "
        f"-S localhost:8080 -t {app_serve}"
    )


def test_wasmer_app_yaml_sets_memory_limit_for_wordpress_phpix(
    tmp_path: Path,
) -> None:
    src_dir = tmp_path / "src"
    src_dir.mkdir()

    runner = WasmerRunner(DummyBuildBackend(tmp_path), src_dir)
    provider_config = load_provider_config(
        WordPressProvider,
        src_dir,
        Config(),
        {"phpix": True},
    )
    runner.prepare_config(provider_config)
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
    assert app_yaml["enable_email"] is True
    assert app_yaml["env"]["PHPIX_PHP_THREADS"] == "4"
    assert app_yaml["annotations"]["shipitcli.com/config"]["phpix"] is True
    assert app_yaml["annotations"]["shipitcli.com/provider"] == "wordpress"
    assert app_yaml["annotations"]["shipitcli.com/version"] == shipit_version
    assert app_yaml["annotations"]["wasmer.io/app-kind"] == "wordpress"
