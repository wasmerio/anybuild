from pathlib import Path

from shipit import cli
from typer.testing import CliRunner


class FakeBuildBackend:
    def __init__(self, *args, **kwargs) -> None:
        pass


class FakeRunner:
    instances = []
    has_after_deploy = True

    def __init__(self, *args, **kwargs) -> None:
        self.calls: list[str] = []
        self.has_after_deploy = type(self).has_after_deploy
        type(self).instances.append(self)

    def has_serve_command(self, command: str) -> bool:
        return command == "after_deploy" and self.has_after_deploy

    def run_serve_command(self, command: str) -> None:
        self.calls.append(command)


runner = CliRunner()


def test_serve_runs_after_deploy_before_start(
    tmp_path: Path,
    monkeypatch,
) -> None:
    FakeRunner.instances.clear()
    FakeRunner.has_after_deploy = True
    monkeypatch.setattr(cli, "LocalBuildBackend", FakeBuildBackend)
    monkeypatch.setattr(cli, "LocalRunner", FakeRunner)

    result = runner.invoke(cli.app, ["serve", str(tmp_path), "--after-deploy"])

    assert result.exit_code == 0, result.output
    assert FakeRunner.instances[-1].calls == ["after_deploy", "start"]


def test_serve_skips_after_deploy_when_missing(
    tmp_path: Path,
    monkeypatch,
) -> None:
    FakeRunner.instances.clear()
    FakeRunner.has_after_deploy = False
    monkeypatch.setattr(cli, "LocalBuildBackend", FakeBuildBackend)
    monkeypatch.setattr(cli, "LocalRunner", FakeRunner)

    result = runner.invoke(cli.app, ["serve", str(tmp_path), "--after-deploy"])

    assert result.exit_code == 0, result.output
    assert FakeRunner.instances[-1].calls == ["start"]


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
