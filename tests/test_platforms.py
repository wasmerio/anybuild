import json
import inspect
import os
from pathlib import Path

from typer.testing import CliRunner

from shipit import cli
from shipit.platforms import apply_platform_config, detect_platform_config
from shipit.providers.base import Config


def test_procfile_platform_prefers_web_and_maps_release(tmp_path: Path) -> None:
    (tmp_path / "Procfile").write_text(
        "worker: npm run worker\n"
        "release: npm run migrate\n"
        "web: npm start\n"
    )

    platform_config = detect_platform_config(tmp_path)
    base_config = apply_platform_config(Config(), platform_config)

    assert platform_config is not None
    assert platform_config.platform == "procfile"
    assert platform_config.selected_entry is not None
    assert platform_config.selected_entry.name == "web"
    assert base_config.commands.start == "npm start"
    assert base_config.commands.after_deploy == "npm run migrate"


def test_render_platform_maps_root_and_commands(tmp_path: Path) -> None:
    (tmp_path / "apps" / "web").mkdir(parents=True)
    (tmp_path / "render.yaml").write_text(
        "services:\n"
        "  - type: web\n"
        "    name: web\n"
        "    rootDir: apps/web\n"
        "    buildCommand: npm run build:render\n"
        "    startCommand: npm run start:render\n"
        "    preDeployCommand: npm run migrate\n"
        "  - type: worker\n"
        "    name: queue\n"
        "    startCommand: npm run worker\n"
    )

    platform_config = detect_platform_config(tmp_path)
    base_config = apply_platform_config(Config(), platform_config)

    assert platform_config is not None
    assert platform_config.platform == "render"
    assert platform_config.selected_entry is not None
    assert platform_config.selected_entry.name == "web"
    assert platform_config.selected_entry.root == "apps/web"
    assert base_config.commands.build == "npm run build:render"
    assert base_config.commands.start == "npm run start:render"
    assert base_config.commands.after_deploy == "npm run migrate"
    assert platform_config.entries[1].unsupported_reason is not None


def test_railway_platform_detects_nested_config(tmp_path: Path) -> None:
    app_path = tmp_path / "apps" / "api"
    app_path.mkdir(parents=True)
    (app_path / "railway.toml").write_text(
        "[build]\n"
        'buildCommand = "npm run build:railway"\n'
        "\n"
        "[deploy]\n"
        'startCommand = "npm run start:railway"\n'
        'preDeployCommand = ["npm run db:push", "npm run seed"]\n'
    )

    platform_config = detect_platform_config(tmp_path)
    base_config = apply_platform_config(Config(), platform_config)

    assert platform_config is not None
    assert platform_config.platform == "railway"
    assert platform_config.selected_entry is not None
    assert platform_config.selected_entry.root == "apps/api"
    assert base_config.commands.build == "npm run build:railway"
    assert base_config.commands.start == "npm run start:railway"
    assert base_config.commands.after_deploy == "npm run db:push && npm run seed"


def test_vercel_platform_uses_first_web_target_when_multiple_exist(
    tmp_path: Path,
) -> None:
    (tmp_path / "vercel.json").write_text(
        json.dumps(
            {
                "services": {
                    "web": {"type": "web", "root": "apps/web"},
                    "api": {"type": "web", "root": "apps/api"},
                }
            }
        )
    )

    platform_config = detect_platform_config(tmp_path)

    assert platform_config is not None
    assert platform_config.selected_entry is not None
    assert platform_config.selected_entry.name == "web"
    assert platform_config.warnings == [
        "Multiple runnable platform entries were detected; using the "
        "first one: web"
    ]


def test_platform_config_does_not_change_project_root(
    tmp_path: Path,
) -> None:
    (tmp_path / "vercel.json").write_text(
        json.dumps(
            {
                "services": {
                    "web": {
                        "type": "web",
                        "root": "apps/web",
                        "runtime": "node",
                        "entrypoint": "server.js",
                    }
                }
            }
        )
    )

    platform_config = detect_platform_config(tmp_path)
    base_config = apply_platform_config(Config(), platform_config)

    assert platform_config is not None
    assert platform_config.selected_entry is not None
    assert platform_config.selected_entry.root == "apps/web"
    assert base_config.commands.start == "node server.js"


def test_most_recent_platform_config_wins(tmp_path: Path) -> None:
    procfile_path = tmp_path / "Procfile"
    procfile_path.write_text("web: npm start\n")
    render_path = tmp_path / "render.yaml"
    render_path.write_text(
        "services:\n"
        "  - type: web\n"
        "    name: web\n"
        "    startCommand: npm run render-start\n"
    )
    os.utime(procfile_path, (100, 100))
    os.utime(render_path, (200, 200))

    platform_config = detect_platform_config(tmp_path)
    base_config = apply_platform_config(Config(), platform_config)

    assert platform_config is not None
    assert platform_config.platform == "render"
    assert base_config.commands.start == "npm run render-start"


def test_plan_does_not_output_platform_config(tmp_path: Path) -> None:
    (tmp_path / "vercel.json").write_text(
        json.dumps(
            {
                "services": {
                    "web": {
                        "type": "web",
                        "buildCommand": "npm run build:vercel",
                    }
                }
            }
        )
    )
    (tmp_path / "package.json").write_text(
        json.dumps(
            {
                "scripts": {
                    "start": "node server.js",
                }
            }
        )
    )
    (tmp_path / "server.js").write_text("console.log('ok')\n")

    result = CliRunner().invoke(
        cli.app,
        ["plan", str(tmp_path), "--regenerate", "--out", str(tmp_path / "plan.json")],
    )

    assert result.exit_code == 0, result.output
    plan = json.loads((tmp_path / "plan.json").read_text())
    assert "platform" not in plan
    assert plan["config"]["commands"]["build"] == "npm run build:vercel"


def test_run_uses_project_path_even_with_platform_root(
    tmp_path: Path,
    monkeypatch,
) -> None:
    app_path = tmp_path / "apps" / "web"
    app_path.mkdir(parents=True)
    (tmp_path / "vercel.json").write_text(
        json.dumps(
            {
                "services": {
                    "web": {
                        "type": "web",
                        "root": "apps/web",
                    }
                }
            }
        )
    )

    seen_paths: list[Path] = []

    class FakeBuildBackend:
        def __init__(self, path: Path, *args, **kwargs) -> None:
            seen_paths.append(path)

    class FakeRunner:
        def __init__(self, _build_backend, path: Path) -> None:
            seen_paths.append(path)

        def has_serve_command(self, command: str) -> bool:
            return True

        def run_serve_command(self, command: str, volume_mappings=None) -> None:
            pass

    monkeypatch.setattr(cli, "LocalBuildBackend", FakeBuildBackend)
    monkeypatch.setattr(cli, "LocalRunner", FakeRunner)

    result = CliRunner().invoke(cli.app, ["run", str(tmp_path), "--start"])

    assert result.exit_code == 0, result.output
    assert seen_paths == [tmp_path, tmp_path]


def test_platforms_do_not_import_runners() -> None:
    import shipit.platforms as platforms

    source = inspect.getsource(platforms)

    assert "shipit.runners" not in source
    assert "WasmerRunner" not in source
    assert "LocalRunner" not in source
