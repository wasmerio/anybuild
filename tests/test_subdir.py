import json
from pathlib import Path

import pytest
from typer.testing import CliRunner

from shipit import cli
from shipit.builders.local import LocalBuildBackend
from shipit.cli import evaluate_shipit
from shipit.generator import load_provider, load_provider_config
from shipit.providers.base import Config
from shipit.runners.local import LocalRunner
from shipit.shipit_types import CopyStep, RunStep, WorkdirStep


runner = CliRunner()


def _write_node_workspace(path: Path) -> Path:
    app_path = path / "apps" / "site"
    app_path.mkdir(parents=True)
    (app_path / "package.json").write_text(
        json.dumps(
            {
                "scripts": {
                    "start": "node server.js",
                },
                "dependencies": {
                    "express": "^5.1.0",
                },
            }
        )
        + "\n"
    )
    (app_path / "package-lock.json").write_text("{}\n")
    (app_path / "server.js").write_text("console.log('ok')\n")
    return app_path


def _write_static_workspace(path: Path) -> Path:
    app_path = path / "apps" / "site"
    public_path = app_path / "public"
    public_path.mkdir(parents=True)
    (public_path / "index.html").write_text("<h1>ok</h1>\n")
    return app_path


def _write_go_workspace(path: Path) -> Path:
    app_path = path / "apps" / "site"
    app_path.mkdir(parents=True)
    (app_path / "go.mod").write_text("module example.com/site\n\ngo 1.25\n")
    (app_path / "main.go").write_text("package main\nfunc main() {}\n")
    return app_path


def _write_python_workspace(path: Path) -> Path:
    app_path = path / "apps" / "site"
    app_path.mkdir(parents=True)
    (app_path / "requirements.txt").write_text("click==8.1.7\n")
    (app_path / "main.py").write_text("print('ok')\n")
    return app_path


def _write_pnpm_static_workspace(path: Path) -> Path:
    app_path = path / "apps" / "site"
    ui_path = path / "packages" / "ui"
    shared_path = path / "packages" / "shared"
    app_path.mkdir(parents=True)
    ui_path.mkdir(parents=True)
    shared_path.mkdir(parents=True)
    (path / "package.json").write_text(
        json.dumps({"name": "workspace", "private": True}) + "\n"
    )
    (path / "package-lock.json").write_text("{}\n")
    (path / "pnpm-workspace.yaml").write_text(
        "packages:\n  - apps/*\n  - packages/*\n"
    )
    (app_path / "package.json").write_text(
        json.dumps(
            {
                "name": "@workspace/site",
                "scripts": {"build": "astro build"},
                "dependencies": {
                    "astro": "^6.4.4",
                    "@workspace/shared": "workspace:*",
                    "@workspace/ui": "workspace:*",
                },
            }
        )
        + "\n"
    )
    (ui_path / "package.json").write_text(
        json.dumps({"name": "@workspace/ui"}) + "\n"
    )
    (shared_path / "package.json").write_text(
        json.dumps({"name": "@workspace/shared"}) + "\n"
    )
    return app_path


def _write_pnpm_node_workspace(path: Path) -> Path:
    app_path = path / "apps" / "api"
    app_path.mkdir(parents=True)
    (path / "package.json").write_text(
        json.dumps({"name": "workspace", "private": True}) + "\n"
    )
    (path / "pnpm-workspace.yaml").write_text("packages:\n  - apps/*\n")
    (app_path / "package.json").write_text(
        json.dumps(
            {
                "name": "@workspace/api",
                "scripts": {
                    "build": "astro build",
                    "start": "node dist/server/entry.mjs",
                },
                "dependencies": {
                    "@astrojs/node": "10.1.3",
                    "astro": "^6.4.4",
                },
            }
        )
        + "\n"
    )
    (app_path / "server.js").write_text("console.log('ok')\n")
    return app_path


def test_generate_subdir_shipit_uses_plain_mounts(tmp_path: Path) -> None:
    _write_node_workspace(tmp_path)

    result = runner.invoke(
        cli.app,
        ["generate", str(tmp_path), "--subdir=apps/site"],
    )

    assert result.exit_code == 0, result.output
    shipit = (tmp_path / "Shipit").read_text()
    assert not (tmp_path / "apps" / "site" / "Shipit").exists()
    assert 'app = mount("app")' in shipit
    assert "subdir=" not in shipit
    assert 'app_subdir = "apps/site"' in shipit
    assert 'cwd=app.serve_path,' in shipit
    assert 'workdir("{}/{}".format(build.path, app_subdir))' in shipit
    assert 'copy(".", ".", ignore=[".git", "node_modules"])' in shipit
    assert 'inputs=["package.json"]' not in shipit
    assert (
        'copy("{}/package-lock.json".format(app_subdir), "package-lock.json")'
        in shipit
    )
    assert 'copy(app_subdir, ".", ignore=[' not in shipit
    assert 'run("cp -RL . {}".format(app.path))' in shipit
    assert '"{}/{}".format(app.path, app_subdir)' not in shipit
    assert '"{}/{}".format(app.serve_path, app_subdir)' not in shipit


def test_generate_subdir_inherits_workspace_package_manager(
    tmp_path: Path,
) -> None:
    _write_pnpm_static_workspace(tmp_path)

    result = runner.invoke(
        cli.app,
        ["generate", str(tmp_path), "--subdir=apps/site"],
    )

    assert result.exit_code == 0, result.output
    shipit = (tmp_path / "Shipit").read_text()
    assert 'pnpm = dep("pnpm", config.pnpm_version)' in shipit
    assert 'run("pnpm install' in shipit
    assert (
        'run("pnpm run build", outputs=[config.static_dir], group="build")'
        in shipit
    )
    assert 'run("npm install"' not in shipit


def test_generate_pnpm_node_subdir_uses_deploy_export(
    tmp_path: Path,
) -> None:
    _write_pnpm_node_workspace(tmp_path)

    result = runner.invoke(
        cli.app,
        ["generate", str(tmp_path), "--subdir=apps/api"],
    )

    assert result.exit_code == 0, result.output
    shipit = (tmp_path / "Shipit").read_text()
    assert 'workdir("{}/{}".format(build.path, app_subdir))' in shipit
    assert 'run("pnpm install' in shipit
    assert 'pnpm_config_inject_workspace_packages="true"' in shipit
    assert "pnpm_config_dedupe_injected_deps" not in shipit
    assert 'pnpm_config_dangerously_allow_all_builds="true"' in shipit
    assert 'run("pnpm run build", outputs=["."], group="build")' in shipit
    assert 'workdir(build.path)' in shipit
    assert (
        'run("pnpm deploy --filter @workspace/api --prod '
        '--config.node-linker=hoisted {}".format(app.path))'
    ) in shipit
    assert 'run("pnpm prune --prod", group="prune")' not in shipit
    assert 'run("pnpm dlx optimize-deps@0.1.1 dist --replace")' in shipit
    assert 'run("cp -R .' not in shipit
    assert 'run("cp -RL .' not in shipit


def test_generate_subdir_static_provider_keeps_serve_mount_flat(
    tmp_path: Path,
) -> None:
    _write_static_workspace(tmp_path)

    result = runner.invoke(
        cli.app,
        ["generate", str(tmp_path), "--subdir=apps/site"],
    )

    assert result.exit_code == 0, result.output
    shipit = (tmp_path / "Shipit").read_text()
    assert 'workdir(static_app.path)' in shipit
    assert 'copy("{}/public".format(app_subdir), ".", ignore=[".git"])' in shipit
    assert '"{}/{}".format(static_app.serve_path, app_subdir)' not in shipit
    assert 'static_"{}/{}".format(app.serve_path' not in shipit


def test_generate_subdir_rewrites_active_build_mount_paths(
    tmp_path: Path,
) -> None:
    _write_go_workspace(tmp_path)

    result = runner.invoke(
        cli.app,
        ["generate", str(tmp_path), "--subdir=apps/site"],
    )

    assert result.exit_code == 0, result.output
    shipit = (tmp_path / "Shipit").read_text()
    assert 'copy(".", ".", ignore=[".git"])' in shipit
    assert "node_modules" not in shipit
    assert 'GOPATH="{}/{}".format(temp.path, app_subdir)' in shipit
    assert (
        'run("cp {} {}/{}".format(config.serve_binary, app.path, '
        'config.serve_binary))'
    ) in shipit
    assert '"{}/{}".format(app.path, app_subdir)' not in shipit


def test_generate_python_subdir_uses_temp_build_mount(
    tmp_path: Path,
) -> None:
    _write_python_workspace(tmp_path)

    result = runner.invoke(
        cli.app,
        ["generate", str(tmp_path), "--subdir=apps/site"],
    )

    assert result.exit_code == 0, result.output
    shipit = (tmp_path / "Shipit").read_text()
    assert 'temp = mount("temp")' in shipit
    assert 'app = mount("app")' in shipit
    assert 'cwd=app.serve_path,' in shipit
    assert 'workdir(temp.path)' in shipit
    assert 'workdir("{}/{}".format(temp.path, app_subdir))' in shipit
    assert 'run("uv add -r requirements.txt ' in shipit
    assert "inputs=[\"requirements.txt\"]" not in shipit
    assert 'run("cp -R . {}".format(app.path))' in shipit
    assert '"{}/{}".format(app.path, app_subdir)' not in shipit


def test_subdir_shipit_evaluates_to_subdir_build_and_runtime_paths(
    tmp_path: Path,
) -> None:
    app_path = _write_node_workspace(tmp_path)
    base_config = Config()
    base_config.commands.enrich_from_path(app_path)
    provider_cls = load_provider(app_path, base_config)
    provider_config = load_provider_config(provider_cls, app_path, base_config)
    provider_config.app_subdir = "apps/site"
    provider = provider_cls(app_path, provider_config)
    (tmp_path / "Shipit").write_text(
        cli.generate_shipit(app_path, provider, subdir="apps/site")
    )
    build_backend = LocalBuildBackend(tmp_path, tmp_path / "assets")
    local_runner = LocalRunner(build_backend, tmp_path)

    _ctx, serve = evaluate_shipit(
        tmp_path / "Shipit",
        build_backend,
        local_runner,
        provider_config,
    )

    assert serve.cwd
    assert serve.cwd.endswith("/app")
    assert isinstance(serve.build[1], WorkdirStep)
    assert serve.build[1].path == build_backend.get_build_mount_path("build")
    assert isinstance(serve.build[2], CopyStep)
    assert serve.build[2].source == "."
    assert serve.build[2].target == "."
    assert serve.build[2].ignore == [".git", "node_modules"]
    assert isinstance(serve.build[3], WorkdirStep)
    assert serve.build[3].path == (
        build_backend.get_build_mount_path("build") / "apps" / "site"
    )
    install_step = next(
        step
        for step in serve.build
        if isinstance(step, RunStep) and step.group == "install"
    )
    assert install_step.inputs is None


def test_build_recovers_subdir_from_generated_shipit(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _write_node_workspace(tmp_path)
    generate_result = runner.invoke(
        cli.app,
        ["generate", str(tmp_path), "--subdir=apps/site"],
    )
    assert generate_result.exit_code == 0, generate_result.output

    build_backend_instances = []
    runner_instances = []

    class FakeBuildBackend:
        def __init__(self, src_dir: Path, assets_path: Path) -> None:
            self.src_dir = src_dir
            self.assets_path = assets_path
            self.runtime_path = None
            build_backend_instances.append(self)

        def get_build_mount_path(self, name: str) -> Path:
            return self.src_dir / ".shipit" / "fake" / "build" / name

        def get_artifact_mount_path(self, name: str) -> Path:
            return self.get_build_mount_path(name)

        def get_volume_path(self, name: str) -> Path:
            return self.src_dir / ".shipit" / "volumes" / name

        def get_runtime_path(self) -> str | None:
            return self.runtime_path

        def build(self, name, env, mounts, steps) -> None:
            self.build_args = {
                "name": name,
                "env": env,
                "mounts": mounts,
                "steps": steps,
            }

    class FakeRunner:
        def __init__(self, build_backend, src_dir: Path) -> None:
            self.build_backend = build_backend
            self.src_dir = src_dir
            runner_instances.append(self)

        def prepare_config(self, provider_config):
            return provider_config

        def prepare_build_steps(self, build_steps):
            return build_steps

        def get_serve_mount_path(self, name: str) -> Path:
            return self.build_backend.get_artifact_mount_path(name)

        def build(self, serve) -> None:
            self.serve = serve

        def prepare(self, env, prepare) -> None:
            self.prepare_args = (env, prepare)

    monkeypatch.setattr(cli, "LocalBuildBackend", FakeBuildBackend)
    monkeypatch.setattr(cli, "LocalRunner", FakeRunner)

    result = runner.invoke(cli.app, ["build", str(tmp_path)])

    assert result.exit_code == 0, result.output
    assert build_backend_instances[-1].src_dir == tmp_path.resolve()
    serve = runner_instances[-1].serve
    assert serve.cwd.endswith("/app")
    install_step = next(
        step
        for step in build_backend_instances[-1].build_args["steps"]
        if isinstance(step, RunStep) and step.group == "install"
    )
    assert install_step.inputs is None


def test_plan_accepts_subdir_and_reports_app_provider(tmp_path: Path) -> None:
    _write_node_workspace(tmp_path)

    result = runner.invoke(
        cli.app,
        ["plan", str(tmp_path), "--subdir=apps/site", "--regenerate"],
    )

    assert result.exit_code == 0, result.output
    output = json.loads(result.stdout)
    assert output["provider"] == "node"


def test_subdir_env_files_override_workspace_env(tmp_path: Path) -> None:
    app_path = _write_node_workspace(tmp_path)
    (tmp_path / ".env").write_text("SHARED=root\nROOT_ONLY=root\n")
    (tmp_path / ".env.production").write_text(
        "SHARED=root-prod\nROOT_PROD=root-prod\nGENERIC=workspace-prod\n"
    )
    (app_path / ".env").write_text("SHARED=app\nAPP_ONLY=app\nGENERIC=app\n")
    (app_path / ".env.production").write_text(
        "SHARED=app-prod\nAPP_PROD=app-prod\n"
    )
    project_paths = cli.resolve_project_paths(tmp_path, Path("apps/site"))
    env = {}

    cli.load_env_files(project_paths, "production", env)

    assert env["SHARED"] == "app-prod"
    assert env["GENERIC"] == "app"
    assert env["ROOT_ONLY"] == "root"
    assert env["ROOT_PROD"] == "root-prod"
    assert env["APP_ONLY"] == "app"
    assert env["APP_PROD"] == "app-prod"


def test_auto_passes_subdir_to_generate_and_build(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _write_node_workspace(tmp_path)
    calls = {}

    def fake_generate(path: Path, **kwargs) -> None:
        calls["generate"] = (path, kwargs)
        (path / "Shipit").write_text('app_subdir = "apps/site"\n')

    def fake_build(path: Path, **kwargs) -> None:
        calls["build"] = (path, kwargs)

    monkeypatch.setattr(cli, "generate", fake_generate)
    monkeypatch.setattr(cli, "build", fake_build)

    result = runner.invoke(cli.app, ["auto", str(tmp_path), "--subdir=apps/site"])

    assert result.exit_code == 0, result.output
    assert calls["generate"][0] == tmp_path.resolve()
    assert calls["generate"][1]["subdir"] == Path("apps/site")
    assert calls["build"][0] == tmp_path.resolve()
    assert calls["build"][1]["subdir"] == Path("apps/site")


@pytest.mark.parametrize(
    "subdir",
    [
        "/tmp/app",
        "../outside",
        "missing",
    ],
)
def test_subdir_validation_rejects_invalid_paths(
    tmp_path: Path,
    subdir: str,
) -> None:
    _write_node_workspace(tmp_path)

    with pytest.raises(ValueError):
        cli.resolve_project_paths(tmp_path, Path(subdir))
