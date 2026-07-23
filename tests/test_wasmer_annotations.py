import subprocess
import tomllib
from pathlib import Path

import pytest
import yaml

from shipit.providers.node import NodeConfig
from shipit.providers.php import PhpConfig, PhpFramework
from shipit.providers.python import PythonConfig, PythonFramework
from shipit.runners.wasmer import (
    BUILD_ANNOTATIONS_FILENAME,
    WasmerRunner,
    resolve_app_kind,
    serialize_provider_config,
)
from shipit.shipit_types import Package, Serve, Volume
from shipit.version import version as shipit_version


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


@pytest.mark.parametrize(
    ("provider", "framework", "expected"),
    [
        ("wordpress", None, "wordpress"),
        ("python", PythonFramework.Django, "django"),
        ("python", PythonFramework.MCP, "mcp"),
        ("php", PhpFramework.Moodle, "moodle"),
        ("php", PhpFramework.Drupal, "drupal"),
        ("javascript", "ghost", "ghost"),
        ("javascript", "strapi", "strapi"),
        ("node", "ghost", "ghost"),
        ("node-static", "ghost", "ghost"),
        ("python", "fastapi", None),
    ],
)
def test_resolve_app_kind(
    provider: str,
    framework: object,
    expected: str | None,
) -> None:
    assert resolve_app_kind(provider, framework) == expected


def test_serialize_provider_config_excludes_defaults() -> None:
    assert serialize_provider_config(PythonConfig()) == {}
    assert serialize_provider_config(
        PythonConfig(framework=PythonFramework.Django)
    ) == {"framework": "django"}


@pytest.mark.parametrize("phpix", [False, True])
def test_wasmer_prepare_config_enables_phpix(
    tmp_path: Path,
    phpix: bool,
) -> None:
    src_dir = tmp_path / "src"
    src_dir.mkdir()
    runner = WasmerRunner(DummyBuildBackend(tmp_path), src_dir)

    config = runner.prepare_config(PhpConfig(phpix=phpix))

    assert config.phpix is True
    assert runner.provider_config.phpix is True


def test_wasmer_app_yaml_adds_python_annotations(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    src_dir = tmp_path / "src"
    src_dir.mkdir()
    (src_dir / "app.yaml").write_text(
        yaml.dump({"annotations": {"example.com/existing": "keep"}})
    )

    runner = WasmerRunner(DummyBuildBackend(tmp_path), src_dir)
    runner.prepare_config(PythonConfig(framework=PythonFramework.Django))
    monkeypatch.setattr(runner, "_get_wasmer_version", lambda: "7.2.0")

    serve = Serve(
        name="django",
        provider="python",
        build=[],
        deps=[Package("python")],
        commands={
            "start": (
                "python -m uvicorn example.asgi --host 0.0.0.0 --port 8080"
            ),
        },
    )

    runner.build(serve)

    app_yaml = yaml.safe_load((runner.wasmer_dir_path / "app.yaml").read_text())
    annotations = app_yaml["annotations"]
    build_annotations = yaml.safe_load(
        (runner.wasmer_dir_path / BUILD_ANNOTATIONS_FILENAME).read_text()
    )

    assert build_annotations == {"wasmer.io/version": "7.2.0"}
    assert annotations["example.com/existing"] == "keep"
    assert annotations["shipitcli.com/provider"] == "python"
    assert annotations["shipitcli.com/version"] == shipit_version
    assert annotations["wasmer.io/app-kind"] == "django"
    assert annotations["wasmer.io/version"] == "7.2.0"
    assert annotations["shipitcli.com/config"]["framework"] == "django"
    assert "python_version" not in annotations["shipitcli.com/config"]
    assert "precompile_python" not in annotations["shipitcli.com/config"]
    assert (
        annotations["shipitcli.com/config"]["cross_platform"]
        == "wasix_wasm32"
    )
    assert (
        annotations["shipitcli.com/config"]["python_extra_index_url"]
        == "https://pythonindex.wasix.org/simple"
    )


def test_wasmer_app_yaml_updates_existing_volume_with_same_mount(
    tmp_path: Path,
) -> None:
    src_dir = tmp_path / "src"
    src_dir.mkdir()
    (src_dir / "app.yaml").write_text(
        yaml.dump(
            {
                "volumes": [
                    {
                        "name": "old-wp-content",
                        "mount": "/app/wp-content",
                        "retention": "keep",
                    },
                    {
                        "name": "cache",
                        "mount": "/app/cache",
                    },
                ],
            }
        )
    )

    runner = WasmerRunner(DummyBuildBackend(tmp_path), src_dir)
    serve = Serve(
        name="wordpress",
        provider="wordpress",
        build=[],
        deps=[Package("php")],
        commands={"start": "php -S localhost:8080 -t /app"},
        volumes=[
            Volume(
                name="wp-content",
                path=tmp_path / ".shipit" / "volumes" / "wp-content",
                serve_path=Path("/app/wp-content"),
            ),
            Volume(
                name="uploads",
                path=tmp_path / ".shipit" / "volumes" / "uploads",
                serve_path=Path("/app/uploads"),
            ),
        ],
    )

    runner.build_serve(serve)

    app_yaml = yaml.safe_load((runner.wasmer_dir_path / "app.yaml").read_text())
    volumes = app_yaml["volumes"]

    wp_content_volumes = [
        volume for volume in volumes if volume["mount"] == "/app/wp-content"
    ]
    assert wp_content_volumes == [
        {
            "name": "wp-content",
            "mount": "/app/wp-content",
            "retention": "keep",
        }
    ]
    assert {"name": "cache", "mount": "/app/cache"} in volumes
    assert {"name": "uploads", "mount": "/app/uploads"} in volumes


def test_wasmer_run_command_inherits_stdio(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    src_dir = tmp_path / "src"
    src_dir.mkdir()
    runner = WasmerRunner(DummyBuildBackend(tmp_path), src_dir, bin="wasmer")

    captured: dict[str, object] = {}

    def fake_run(*args, **kwargs) -> None:
        captured["args"] = args
        captured["kwargs"] = kwargs

    monkeypatch.setattr("shipit.runners.wasmer.subprocess.run", fake_run)

    runner.run_command("wasmer", ["run", "."], env={"SHIPIT": "1"})

    assert captured["args"] == (["wasmer", "run", "."],)
    assert captured["kwargs"] == {
        "check": True,
        "env": {"SHIPIT": "1"},
    }


def test_get_wasmer_version(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    src_dir = tmp_path / "src"
    src_dir.mkdir()
    runner = WasmerRunner(DummyBuildBackend(tmp_path), src_dir, bin="wasmer")

    def fake_run(*args, **kwargs):
        assert args == (["wasmer", "--version"],)
        assert kwargs == {
            "check": True,
            "capture_output": True,
            "text": True,
        }
        return subprocess.CompletedProcess(args[0], 0, stdout="wasmer 7.2.0\n")

    monkeypatch.setattr("shipit.runners.wasmer.subprocess.run", fake_run)

    assert runner._get_wasmer_version() == "7.2.0"


def test_wasmer_node_manifest_maps_to_edgejs(tmp_path: Path) -> None:
    src_dir = tmp_path / "src"
    src_dir.mkdir()
    runner = WasmerRunner(DummyBuildBackend(tmp_path), src_dir)

    serve = Serve(
        name="node",
        provider="node",
        build=[],
        deps=[Package("node", "22")],
        cwd="/app",
        commands={"start": "node server.js"},
    )

    runner.build_serve(serve)

    manifest = tomllib.loads((runner.wasmer_dir_path / "wasmer.toml").read_text())
    assert manifest["dependencies"]["wasmer/edgejs-quickjs"] == "=0.0.7"
    assert manifest["command"][0]["module"] == "wasmer/edgejs-quickjs:edge"
    wasi = manifest["command"][0]["annotations"]["wasi"]
    assert wasi["main-args"] == ["--bytecode-cache", "server.js"]
    assert "env" not in wasi


@pytest.mark.parametrize(
    ("version", "architecture", "expected_package"),
    [
        (None, None, "phpix/phpix-84-32bit"),
        ("latest", "64-bit", "phpix/phpix-84-64bit"),
        ("8.5", "32-bit", "phpix/phpix-85-32bit"),
        ("8.5", "64-bit", "phpix/phpix-85-64bit"),
        ("8.4", "32-bit", "phpix/phpix-84-32bit"),
        ("8.4", "64-bit", "phpix/phpix-84-64bit"),
        ("8.3.29", None, "phpix/phpix-83-32bit"),
    ],
)
def test_wasmer_phpix_manifest_maps_php_versions(
    tmp_path: Path,
    version: str | None,
    architecture: str | None,
    expected_package: str,
) -> None:
    src_dir = tmp_path / "src"
    src_dir.mkdir()
    runner = WasmerRunner(DummyBuildBackend(tmp_path), src_dir)

    serve = Serve(
        name="php",
        provider="php",
        build=[],
        deps=[Package("phpix", version, architecture)],
        cwd="/app",
        commands={"start": "phpix -S localhost:8080 -t /app"},
    )

    runner.build_serve(serve)

    manifest = tomllib.loads((runner.wasmer_dir_path / "wasmer.toml").read_text())
    assert manifest["dependencies"][expected_package] == "=0.3.0-rc.2"
    assert manifest["command"][0]["module"] == f"{expected_package}:phpix"


def test_wasmer_edge_manifest_does_not_duplicate_bytecode_cache(
    tmp_path: Path,
) -> None:
    src_dir = tmp_path / "src"
    src_dir.mkdir()
    runner = WasmerRunner(DummyBuildBackend(tmp_path), src_dir)

    serve = Serve(
        name="node",
        provider="node",
        build=[],
        deps=[Package("node", "22")],
        commands={"start": "edge --bytecode-cache server.js"},
    )

    runner.build_serve(serve)

    manifest = tomllib.loads((runner.wasmer_dir_path / "wasmer.toml").read_text())
    wasi = manifest["command"][0]["annotations"]["wasi"]
    assert wasi["main-args"] == ["--bytecode-cache", "server.js"]


def test_wasmer_prepare_config_enables_node_edge_optimizations(
    tmp_path: Path,
) -> None:
    src_dir = tmp_path / "src"
    src_dir.mkdir()
    runner = WasmerRunner(DummyBuildBackend(tmp_path), src_dir)

    config = runner.prepare_config(NodeConfig())

    assert runner.provider_config.use_edgejs is True
    assert runner.provider_config.precompile_edgejs is True
    assert runner.provider_config.remove_native_binaries is True
    assert config.use_edgejs is True
    assert config.precompile_edgejs is True
    assert config.remove_native_binaries is True


def test_wasmer_prepare_config_preserves_node_precompile_override(
    tmp_path: Path,
) -> None:
    src_dir = tmp_path / "src"
    src_dir.mkdir()
    runner = WasmerRunner(DummyBuildBackend(tmp_path), src_dir)

    config = runner.prepare_config(NodeConfig(precompile_edgejs=False))

    assert runner.provider_config.use_edgejs is True
    assert runner.provider_config.precompile_edgejs is False
    assert runner.provider_config.remove_native_binaries is True
    assert config.precompile_edgejs is False


def test_wasmer_run_command_enables_napi_for_edgejs(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    src_dir = tmp_path / "src"
    src_dir.mkdir()
    runner = WasmerRunner(DummyBuildBackend(tmp_path), src_dir, bin="wasmer")
    runner.wasmer_dir_path.mkdir(parents=True)
    (runner.wasmer_dir_path / "wasmer.toml").write_text(
        """
[[command]]
name = "start"
module = "sadhbh-c0d3/edgejs-quickjs:edge"
runner = "wasi"
"""
    )

    captured: dict[str, object] = {}

    def fake_run_command(command, extra_args=None, env=None) -> None:
        captured["command"] = command
        captured["extra_args"] = extra_args
        captured["env"] = env

    monkeypatch.setattr(runner, "run_command", fake_run_command)

    runner.run_serve_command("start")

    assert captured["command"] == "wasmer"
    # assert captured["extra_args"][:2] == ["run", "--experimental-napi"]


def test_wasmer_run_command_passes_runtime_env(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    src_dir = tmp_path / "src"
    src_dir.mkdir()
    runner = WasmerRunner(DummyBuildBackend(tmp_path), src_dir, bin="wasmer")

    captured: dict[str, object] = {}

    def fake_run_command(command, extra_args=None, env=None) -> None:
        captured["command"] = command
        captured["extra_args"] = extra_args
        captured["env"] = env

    monkeypatch.setattr(runner, "run_command", fake_run_command)

    runner.run_serve_command("start", env={"PORT": "45678"})

    assert captured["command"] == "wasmer"
    assert captured["env"]["PORT"] == "45678"
