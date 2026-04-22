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
        self.calls: list[str] = []
        self.checked: list[str] = []
        self.volume_mappings: list[dict[str, str] | None] = []
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


runner = CliRunner()


def test_serve_runs_after_deploy_before_start(
    tmp_path: Path,
    monkeypatch,
) -> None:
    FakeRunner.instances.clear()
    FakeRunner.available_commands = {"start", "after_deploy"}
    monkeypatch.setattr(cli, "LocalBuildBackend", FakeBuildBackend)
    monkeypatch.setattr(cli, "LocalRunner", FakeRunner)

    result = runner.invoke(cli.app, ["serve", str(tmp_path), "--after-deploy"])

    assert result.exit_code == 0, result.output
    assert FakeRunner.instances[-1].calls == ["after_deploy", "start"]
    assert FakeRunner.instances[-1].checked == ["after_deploy", "start"]
    assert FakeRunner.instances[-1].volume_mappings == [{}, {}]


def test_serve_skips_after_deploy_when_missing(
    tmp_path: Path,
    monkeypatch,
) -> None:
    FakeRunner.instances.clear()
    FakeRunner.available_commands = {"start"}
    monkeypatch.setattr(cli, "LocalBuildBackend", FakeBuildBackend)
    monkeypatch.setattr(cli, "LocalRunner", FakeRunner)

    result = runner.invoke(cli.app, ["serve", str(tmp_path), "--after-deploy"])

    assert result.exit_code == 0, result.output
    assert FakeRunner.instances[-1].calls == ["start"]
    assert FakeRunner.instances[-1].checked == ["after_deploy", "start"]
    assert FakeRunner.instances[-1].volume_mappings == [{}]


def test_serve_runs_custom_commands_without_existence_checks(
    tmp_path: Path,
    monkeypatch,
) -> None:
    FakeRunner.instances.clear()
    FakeRunner.available_commands = set()
    monkeypatch.setattr(cli, "LocalBuildBackend", FakeBuildBackend)
    monkeypatch.setattr(cli, "LocalRunner", FakeRunner)

    result = runner.invoke(
        cli.app,
        ["serve", str(tmp_path), "--no-start", "--run=prepare-db", "--run=warm-cache"],
    )

    assert result.exit_code == 0, result.output
    assert FakeRunner.instances[-1].calls == ["prepare-db", "warm-cache"]
    assert FakeRunner.instances[-1].checked == []
    assert FakeRunner.instances[-1].volume_mappings == [{}, {}]


def test_serve_loads_volume_mappings_from_json(
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

    result = runner.invoke(cli.app, ["serve", str(tmp_path)])

    assert result.exit_code == 0, result.output
    assert FakeRunner.instances[-1].calls == ["start"]
    assert FakeRunner.instances[-1].volume_mappings == [
        {"wp-content": "/app/wp-content"}
    ]


def test_auto_passes_after_deploy_to_serve(
    tmp_path: Path,
    monkeypatch,
) -> None:
    calls: list[dict[str, object]] = []
    (tmp_path / "Shipit").write_text("")

    monkeypatch.setattr(cli, "build", lambda *args, **kwargs: None)
    monkeypatch.setattr(
        cli,
        "serve",
        lambda *args, **kwargs: calls.append(kwargs),
    )

    result = runner.invoke(cli.app, ["auto", str(tmp_path), "--start", "--after-deploy"])

    assert result.exit_code == 0, result.output
    assert calls
    assert calls[0]["after_deploy"] is True
    assert calls[0]["start"] is True


def test_auto_passes_run_commands_to_serve(
    tmp_path: Path,
    monkeypatch,
) -> None:
    calls: list[dict[str, object]] = []
    (tmp_path / "Shipit").write_text("")

    monkeypatch.setattr(cli, "build", lambda *args, **kwargs: None)
    monkeypatch.setattr(
        cli,
        "serve",
        lambda *args, **kwargs: calls.append(kwargs),
    )

    result = runner.invoke(
        cli.app,
        ["auto", str(tmp_path), "--run=prepare-db", "--run=warm-cache"],
    )

    assert result.exit_code == 0, result.output
    assert calls
    assert calls[0]["run_commands"] == ["prepare-db", "warm-cache"]
