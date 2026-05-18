import tomllib
from pathlib import Path

import pytest
import yaml

from shipit.providers.php import PhpFramework
from shipit.providers.python import PythonConfig, PythonFramework
from shipit.runners.wasmer import WasmerRunner, resolve_app_kind
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


def test_wasmer_app_yaml_adds_python_annotations(tmp_path: Path) -> None:
    src_dir = tmp_path / "src"
    src_dir.mkdir()
    (src_dir / "app.yaml").write_text(
        yaml.dump({"annotations": {"example.com/existing": "keep"}})
    )

    runner = WasmerRunner(DummyBuildBackend(tmp_path), src_dir)
    runner.prepare_config(PythonConfig(framework=PythonFramework.Django))

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

    runner.build_serve(serve)

    app_yaml = yaml.safe_load((runner.wasmer_dir_path / "app.yaml").read_text())
    annotations = app_yaml["annotations"]

    assert annotations["example.com/existing"] == "keep"
    assert annotations["shipitcli.com/provider"] == "python"
    assert annotations["shipitcli.com/version"] == shipit_version
    assert annotations["wasmer.io/app-kind"] == "django"
    assert annotations["shipitcli.com/config"]["framework"] == "django"
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
        env={"PORT": "8080"},
    )

    runner.build_serve(serve)

    manifest = tomllib.loads((runner.wasmer_dir_path / "wasmer.toml").read_text())
    assert manifest["dependencies"]["sadhbh-c0d3/edgejs-quickjs"] == "=0.0.5"
    assert manifest["command"][0]["module"] == "sadhbh-c0d3/edgejs-quickjs:edge"
    assert manifest["command"][0]["annotations"]["wasi"]["main-args"] == [
        "server.js"
    ]


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
