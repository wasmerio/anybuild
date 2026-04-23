from pathlib import Path

import pytest

from shipit.builders.local import LocalBuildBackend
from shipit.cli import Ctx
from shipit.runners.local import LocalRunner
from shipit.runners.wasmer import WasmerRunner
from shipit.shipit_types import Package, Serve, Volume
from shipit.volumes import build_volumes, load_volume_mappings


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

    def get_volume_path(self, name: str) -> Path:
        return self.root / ".shipit" / "volumes" / name

    def get_runtime_path(self) -> str | None:
        return None


def test_ctx_volume_uses_shipit_volume_directory(tmp_path: Path) -> None:
    assets_path = tmp_path / "assets"
    assets_path.mkdir()
    build_backend = LocalBuildBackend(tmp_path, assets_path)
    runner = LocalRunner(build_backend, tmp_path)
    ctx = Ctx(build_backend, runner)

    volume_ref = ctx.volume("uploads", "/app/uploads")
    assert volume_ref is not None

    volume = ctx.get_ref(volume_ref["ref"])

    assert volume.path == tmp_path / ".shipit" / "volumes" / "uploads"
    assert not volume.path.exists()


def test_build_volumes_links_runtime_volume_to_host_directory(
    tmp_path: Path,
) -> None:
    assets_path = tmp_path / "assets"
    assets_path.mkdir()
    build_backend = LocalBuildBackend(tmp_path, assets_path)

    target = build_backend.get_artifact_mount_path("app") / "wp-content"
    target.mkdir(parents=True, exist_ok=True)
    (target / "seed.txt").write_text("hello")

    volume = Volume(
        name="wp-content",
        path=build_backend.get_volume_path("wp-content"),
        serve_path=target,
    )
    serve = Serve(
        name="wordpress",
        provider="wordpress",
        build=[],
        deps=[],
        commands={"start": "php -S localhost:8080 -t /app"},
        volumes=[volume],
    )

    mappings = build_volumes(tmp_path, serve)

    assert volume.path.is_dir()
    assert (volume.path / "seed.txt").read_text() == "hello"
    assert target.is_symlink()
    assert target.resolve(strict=False) == volume.path.resolve()
    assert mappings == {"wp-content": str(target)}
    assert load_volume_mappings(tmp_path) == {"wp-content": str(target)}


def test_wasmer_runner_passes_volume_paths_into_wasmer_run(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    src_dir = tmp_path / "src"
    src_dir.mkdir()

    runner = WasmerRunner(DummyBuildBackend(tmp_path), src_dir)
    volume_path = tmp_path / ".shipit" / "volumes" / "wp-content"
    serve = Serve(
        name="wordpress",
        provider="wordpress",
        build=[],
        deps=[Package("php")],
        commands={"start": "php -S localhost:8080 -t /app"},
        volumes=[
            Volume(
                name="wp-content",
                path=volume_path,
                serve_path=Path("/app/wp-content"),
            )
        ],
    )

    captured: dict[str, object] = {}

    def fake_run_command(
        command: str,
        extra_args: list[str] | None = None,
        env: dict[str, str] | None = None,
    ) -> None:
        captured["command"] = command
        captured["extra_args"] = extra_args or []
        captured["env"] = env or {}

    monkeypatch.setattr(runner, "run_command", fake_run_command)

    build_volumes(src_dir, serve)
    runner.run_serve_command(
        "start",
        volume_mappings=load_volume_mappings(src_dir),
    )

    assert volume_path.is_dir()
    assert captured["command"] == "wasmer"
    assert (
        f"--volume={volume_path.absolute()}:/app/wp-content"
        in captured["extra_args"]
    )
