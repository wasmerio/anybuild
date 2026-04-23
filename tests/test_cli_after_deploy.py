from pathlib import Path

from shipit import cli
from typer.testing import CliRunner


class FakeBuildBackend:
    def __init__(self, *args, **kwargs) -> None:
        pass


class FakeRunner:
    instances = []
    available_commands = {"start", "after_deploy"}

    def __init__(self, *args, **kwargs) -> None:
        self.init_kwargs = kwargs
        self.calls: list[str] = []
        self.checked: list[str] = []
        self.volume_mappings: list[dict[str, str] | None] = []
        self.deploy_calls: list[dict[str, str | None]] = []
        self.deploy_config_calls: list[Path] = []
        self.available_commands = set(type(self).available_commands)
        type(self).instances.append(self)

    def has_serve_command(self, command: str) -> bool:
        self.checked.append(command)
        return command in self.available_commands

    def run_serve_command(
        self,
        command: str,
        volume_mappings: dict[str, str] | None = None,
    ) -> None:
        self.calls.append(command)
        self.volume_mappings.append(volume_mappings)

    def deploy(
        self, app_owner: str | None = None, app_name: str | None = None
    ) -> None:
        self.deploy_calls.append(
            {
                "app_owner": app_owner,
                "app_name": app_name,
            }
        )

    def deploy_config(self, config_path: Path) -> None:
        self.deploy_config_calls.append(config_path)


runner = CliRunner()


def test_run_runs_after_deploy_before_start(
    tmp_path: Path,
    monkeypatch,
) -> None:
    FakeRunner.instances.clear()
    FakeRunner.available_commands = {"start", "after_deploy"}
    monkeypatch.setattr(cli, "LocalBuildBackend", FakeBuildBackend)
    monkeypatch.setattr(cli, "LocalRunner", FakeRunner)

    result = runner.invoke(cli.app, ["run", str(tmp_path), "--after-deploy"])

    assert result.exit_code == 0, result.output
    assert FakeRunner.instances[-1].calls == ["after_deploy", "start"]
    assert FakeRunner.instances[-1].checked == ["after_deploy", "start"]
    assert FakeRunner.instances[-1].volume_mappings == [{}, {}]


def test_run_skips_after_deploy_when_missing(
    tmp_path: Path,
    monkeypatch,
) -> None:
    FakeRunner.instances.clear()
    FakeRunner.available_commands = {"start"}
    monkeypatch.setattr(cli, "LocalBuildBackend", FakeBuildBackend)
    monkeypatch.setattr(cli, "LocalRunner", FakeRunner)

    result = runner.invoke(cli.app, ["run", str(tmp_path), "--after-deploy"])

    assert result.exit_code == 0, result.output
    assert FakeRunner.instances[-1].calls == ["start"]
    assert FakeRunner.instances[-1].checked == ["after_deploy", "start"]
    assert FakeRunner.instances[-1].volume_mappings == [{}]


def test_run_runs_custom_commands_without_existence_checks(
    tmp_path: Path,
    monkeypatch,
) -> None:
    FakeRunner.instances.clear()
    FakeRunner.available_commands = set()
    monkeypatch.setattr(cli, "LocalBuildBackend", FakeBuildBackend)
    monkeypatch.setattr(cli, "LocalRunner", FakeRunner)

    result = runner.invoke(
        cli.app,
        [
            "run",
            str(tmp_path),
            "--no-start",
            "--command=prepare-db",
            "-c",
            "warm-cache",
        ],
    )

    assert result.exit_code == 0, result.output
    assert FakeRunner.instances[-1].calls == ["prepare-db", "warm-cache"]
    assert FakeRunner.instances[-1].checked == []
    assert FakeRunner.instances[-1].volume_mappings == [{}, {}]


def test_run_loads_volume_mappings_from_json(
    tmp_path: Path,
    monkeypatch,
) -> None:
    FakeRunner.instances.clear()
    FakeRunner.available_commands = {"start"}
    mappings_dir = tmp_path / ".shipit" / "volumes"
    mappings_dir.mkdir(parents=True)
    (mappings_dir / "mappings.json").write_text(
        '{\n  "wp-content": "/app/wp-content"\n}\n'
    )
    monkeypatch.setattr(cli, "LocalBuildBackend", FakeBuildBackend)
    monkeypatch.setattr(cli, "LocalRunner", FakeRunner)

    result = runner.invoke(cli.app, ["run", str(tmp_path)])

    assert result.exit_code == 0, result.output
    assert FakeRunner.instances[-1].calls == ["start"]
    assert FakeRunner.instances[-1].volume_mappings == [
        {"wp-content": "/app/wp-content"}
    ]


def test_run_merges_cli_volume_mappings(
    tmp_path: Path,
    monkeypatch,
) -> None:
    FakeRunner.instances.clear()
    FakeRunner.available_commands = {"start"}
    mappings_dir = tmp_path / ".shipit" / "volumes"
    mappings_dir.mkdir(parents=True)
    (mappings_dir / "mappings.json").write_text(
        '{\n  "wp-content": "/app/wp-content"\n}\n'
    )
    monkeypatch.setattr(cli, "LocalBuildBackend", FakeBuildBackend)
    monkeypatch.setattr(cli, "LocalRunner", FakeRunner)

    result = runner.invoke(
        cli.app,
        [
            "run",
            str(tmp_path),
            "--volume",
            "uploads:/app/uploads",
            "--volume",
            "wp-content:/app/override",
        ],
    )

    assert result.exit_code == 0, result.output
    assert FakeRunner.instances[-1].calls == ["start"]
    assert FakeRunner.instances[-1].volume_mappings == [
        {
            "wp-content": "/app/override",
            "uploads": "/app/uploads",
        }
    ]


def test_run_passes_wasmer_registry_to_runner(
    tmp_path: Path,
    monkeypatch,
) -> None:
    FakeRunner.instances.clear()
    FakeRunner.available_commands = {"start"}
    monkeypatch.setattr(cli, "LocalBuildBackend", FakeBuildBackend)
    monkeypatch.setattr(cli, "WasmerRunner", FakeRunner)

    result = runner.invoke(
        cli.app,
        [
            "run",
            str(tmp_path),
            "--wasmer",
            "--wasmer-registry",
            "wasmer.io",
        ],
    )

    assert result.exit_code == 0, result.output
    assert FakeRunner.instances[-1].calls == ["start"]
    assert FakeRunner.instances[-1].init_kwargs["registry"] == "wasmer.io"


def test_auto_passes_after_deploy_to_run(
    tmp_path: Path,
    monkeypatch,
) -> None:
    calls: list[dict[str, object]] = []
    (tmp_path / "Shipit").write_text("")

    monkeypatch.setattr(cli, "build", lambda *args, **kwargs: None)
    monkeypatch.setattr(
        cli,
        "run",
        lambda *args, **kwargs: calls.append(kwargs),
    )

    result = runner.invoke(cli.app, ["auto", str(tmp_path), "--start", "--after-deploy"])

    assert result.exit_code == 0, result.output
    assert calls
    assert calls[0]["after_deploy"] is True
    assert calls[0]["start"] is True


def test_auto_passes_commands_to_run(
    tmp_path: Path,
    monkeypatch,
) -> None:
    calls: list[dict[str, object]] = []
    (tmp_path / "Shipit").write_text("")

    monkeypatch.setattr(cli, "build", lambda *args, **kwargs: None)
    monkeypatch.setattr(
        cli,
        "run",
        lambda *args, **kwargs: calls.append(kwargs),
    )

    result = runner.invoke(
        cli.app,
        ["auto", str(tmp_path), "--command=prepare-db", "-c", "warm-cache"],
    )

    assert result.exit_code == 0, result.output
    assert calls
    assert calls[0]["command_names"] == ["prepare-db", "warm-cache"]


def test_auto_passes_volume_specs_to_run(
    tmp_path: Path,
    monkeypatch,
) -> None:
    calls: list[dict[str, object]] = []
    (tmp_path / "Shipit").write_text("")

    monkeypatch.setattr(cli, "build", lambda *args, **kwargs: None)
    monkeypatch.setattr(
        cli,
        "run",
        lambda *args, **kwargs: calls.append(kwargs),
    )

    result = runner.invoke(
        cli.app,
        [
            "auto",
            str(tmp_path),
            "--volume",
            "uploads:/app/uploads",
            "--volume",
            "cache:/app/cache",
        ],
    )

    assert result.exit_code == 0, result.output
    assert calls
    assert calls[0]["volume_specs"] == [
        "uploads:/app/uploads",
        "cache:/app/cache",
    ]


def test_deploy_calls_wasmer_runner_deploy(
    tmp_path: Path,
    monkeypatch,
) -> None:
    FakeRunner.instances.clear()
    monkeypatch.setattr(cli, "LocalBuildBackend", FakeBuildBackend)
    monkeypatch.setattr(cli, "WasmerRunner", FakeRunner)

    result = runner.invoke(
        cli.app,
        [
            "deploy",
            str(tmp_path),
            "--wasmer-registry",
            "wasmer.io",
            "--wasmer-token",
            "token",
            "--wasmer-app-owner",
            "acme",
            "--wasmer-app-name",
            "blog",
        ],
    )

    assert result.exit_code == 0, result.output
    assert FakeRunner.instances[-1].init_kwargs["registry"] == "wasmer.io"
    assert FakeRunner.instances[-1].init_kwargs["token"] == "token"
    assert FakeRunner.instances[-1].deploy_calls == [
        {
            "app_owner": "acme",
            "app_name": "blog",
        }
    ]
    assert FakeRunner.instances[-1].deploy_config_calls == []


def test_deploy_calls_wasmer_runner_deploy_config(
    tmp_path: Path,
    monkeypatch,
) -> None:
    FakeRunner.instances.clear()
    monkeypatch.setattr(cli, "LocalBuildBackend", FakeBuildBackend)
    monkeypatch.setattr(cli, "WasmerRunner", FakeRunner)
    config_path = tmp_path / "deploy.json"

    result = runner.invoke(
        cli.app,
        [
            "deploy",
            str(tmp_path),
            "--no-wasmer-deploy",
            "--wasmer-deploy-config",
            str(config_path),
        ],
    )

    assert result.exit_code == 0, result.output
    assert FakeRunner.instances[-1].deploy_calls == []
    assert FakeRunner.instances[-1].deploy_config_calls == [config_path]
