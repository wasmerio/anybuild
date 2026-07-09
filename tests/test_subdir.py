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
from shipit.shipit_types import CopyStep, EnvStep, RunStep, WorkdirStep


runner = CliRunner()


def _evaluate_subdir_shipit(tmp_path: Path, app_path: Path, subdir: str = "apps/site"):
    """Evaluate the generated Shipit.<subdir> file the way the CLI would."""
    shipit_file = _subdir_shipit_file(tmp_path, subdir)
    base_config = Config()
    base_config.commands.enrich_from_path(app_path)
    provider_cls = load_provider(app_path, base_config)
    provider_config = load_provider_config(provider_cls, app_path, base_config)
    project_paths = cli.ProjectPaths(tmp_path, app_path, subdir)
    cli.apply_subdir_provider_config(project_paths, provider_config)
    cli.apply_subdir_workspace_config(project_paths, provider_config)
    build_backend = LocalBuildBackend(tmp_path, tmp_path / "assets")
    local_runner = LocalRunner(build_backend, tmp_path)
    ctx, serve = evaluate_shipit(
        shipit_file, build_backend, local_runner, provider_config
    )
    return build_backend, ctx, serve, provider_config


def _run_commands(serve) -> list[str]:
    return [step.command for step in serve.build if isinstance(step, RunStep)]


def _subdir_shipit_file(path: Path, subdir: str = "apps/site") -> Path:
    return cli.default_shipit_path(
        cli.resolve_project_paths(path, Path(subdir))
    )


def _write_node_app(path: Path, subdir: str) -> Path:
    app_path = path / subdir
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


def _write_node_workspace(path: Path) -> Path:
    return _write_node_app(path, "apps/site")


def _patch_fake_build_runner(monkeypatch: pytest.MonkeyPatch):
    build_backend_instances = []
    runner_instances = []

    class FakeBuildBackend:
        def __init__(
            self,
            src_dir: Path,
            assets_path: Path,
            shipit_dir: Path | None = None,
        ) -> None:
            self.src_dir = src_dir
            self.assets_path = assets_path
            self.shipit_dir = shipit_dir or src_dir / ".shipit"
            self.runtime_path = None
            build_backend_instances.append(self)

        def get_build_mount_path(self, name: str) -> Path:
            return self.shipit_dir / "fake" / "build" / name

        def get_artifact_mount_path(self, name: str) -> Path:
            return self.get_build_mount_path(name)

        def get_volume_path(self, name: str) -> Path:
            return self.shipit_dir / "volumes" / name

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
        def __init__(
            self,
            build_backend,
            src_dir: Path,
            shipit_dir: Path | None = None,
        ) -> None:
            self.build_backend = build_backend
            self.src_dir = src_dir
            self.shipit_dir = shipit_dir or src_dir / ".shipit"
            self.calls = []
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

        def has_serve_command(self, command: str) -> bool:
            return True

        def run_serve_command(
            self,
            command: str,
            volume_mappings=None,
            env=None,
        ) -> None:
            self.calls.append(
                {
                    "command": command,
                    "volume_mappings": volume_mappings,
                    "env": env,
                }
            )

    monkeypatch.setattr(cli, "LocalBuildBackend", FakeBuildBackend)
    monkeypatch.setattr(cli, "LocalRunner", FakeRunner)
    return build_backend_instances, runner_instances


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
    shipit_file = _subdir_shipit_file(tmp_path)
    shipit = shipit_file.read_text()
    assert shipit_file.name == "Shipit.apps-site"
    assert not (tmp_path / "Shipit").exists()
    assert not (tmp_path / "apps" / "site" / "Shipit").exists()
    assert 'app_subdir = "apps/site"' in shipit

    build_backend, _ctx, serve, _config = _evaluate_subdir_shipit(
        tmp_path, tmp_path / "apps" / "site"
    )
    build_path = build_backend.get_build_mount_path("build")
    app_path = build_backend.get_build_mount_path("app")

    # Staged in the build mount, entering the subdir; served from the flat app.
    workdirs = [step.path for step in serve.build if isinstance(step, WorkdirStep)]
    assert workdirs == [build_path, build_path / "apps" / "site"]
    assert any(
        isinstance(step, CopyStep)
        and step.source == "."
        and step.ignore == [".git", "node_modules"]
        for step in serve.build
    )
    # The workspace lockfile is staged from the subdir path.
    assert any(
        isinstance(step, CopyStep)
        and step.source == "apps/site/package-lock.json"
        and step.target == "package-lock.json"
        for step in serve.build
    )
    install_step = next(
        step
        for step in serve.build
        if isinstance(step, RunStep) and step.command.startswith("npm install")
    )
    assert install_step.inputs is None
    assert f"cp -RL . {app_path}" in _run_commands(serve)
    assert serve.cwd
    assert serve.cwd.endswith("/app")
    assert [mount.name for mount in (serve.mounts or [])] == ["app"]


def test_generate_subdir_shipit_files_do_not_overwrite(
    tmp_path: Path,
) -> None:
    _write_node_app(tmp_path, "apps/dashboard")
    _write_node_app(tmp_path, "apps/site")
    _write_node_app(tmp_path, "apps/docs")

    for subdir in ["apps/dashboard", "apps/site", "apps/docs"]:
        result = runner.invoke(
            cli.app,
            ["generate", str(tmp_path), f"--subdir={subdir}"],
        )
        assert result.exit_code == 0, result.output

    dashboard = (tmp_path / "Shipit.apps-dashboard").read_text()
    site = (tmp_path / "Shipit.apps-site").read_text()
    docs = (tmp_path / "Shipit.apps-docs").read_text()

    assert 'app_subdir = "apps/dashboard"' in dashboard
    assert 'app_subdir = "apps/site"' in site
    assert 'app_subdir = "apps/docs"' in docs
    assert not (tmp_path / "Shipit").exists()


def test_generate_subdir_inherits_workspace_package_manager(
    tmp_path: Path,
) -> None:
    _write_pnpm_static_workspace(tmp_path)

    result = runner.invoke(
        cli.app,
        ["generate", str(tmp_path), "--subdir=apps/site"],
    )

    assert result.exit_code == 0, result.output

    _backend, ctx, serve, config = _evaluate_subdir_shipit(
        tmp_path, tmp_path / "apps" / "site"
    )
    # The workspace package manager (pnpm) wins over the app-level default.
    assert "pnpm" in ctx.packages
    commands = _run_commands(serve)
    assert any(command.startswith("pnpm install") for command in commands)
    assert not any(command.startswith("npm install") for command in commands)
    build_step = next(
        step
        for step in serve.build
        if isinstance(step, RunStep) and step.command == "pnpm run build"
    )
    assert build_step.outputs == [config.static_dir]


def test_generate_pnpm_node_subdir_uses_deploy_export(
    tmp_path: Path,
) -> None:
    _write_pnpm_node_workspace(tmp_path)

    result = runner.invoke(
        cli.app,
        ["generate", str(tmp_path), "--subdir=apps/api"],
    )

    assert result.exit_code == 0, result.output

    build_backend, _ctx, serve, _config = _evaluate_subdir_shipit(
        tmp_path, tmp_path / "apps" / "api", subdir="apps/api"
    )
    build_path = build_backend.get_build_mount_path("build")
    app_path = build_backend.get_build_mount_path("app")
    commands = _run_commands(serve)

    assert any(command.startswith("pnpm install") for command in commands)
    env_step = next(step for step in serve.build if isinstance(step, EnvStep))
    assert env_step.variables.get("pnpm_config_inject_workspace_packages") == "true"
    assert env_step.variables.get("pnpm_config_dangerously_allow_all_builds") == "true"
    assert "pnpm_config_dedupe_injected_deps" not in env_step.variables
    build_step = next(
        step
        for step in serve.build
        if isinstance(step, RunStep) and step.command == "pnpm run build"
    )
    assert build_step.outputs == ["."]
    # pnpm deploy exports the app instead of cp; prune is skipped.
    workdirs = [step.path for step in serve.build if isinstance(step, WorkdirStep)]
    assert workdirs == [
        build_path,
        build_path / "apps" / "api",
        build_path,
        app_path,
    ]
    assert (
        f"pnpm deploy --filter @workspace/api --prod --config.node-linker=hoisted {app_path}"
        in commands
    )
    assert "pnpm prune --prod" not in commands
    assert "pnpm dlx optimize-deps@0.1.1 dist --replace" in commands
    assert not any(command.startswith("cp -R") for command in commands)


def test_generate_subdir_static_provider_keeps_serve_mount_flat(
    tmp_path: Path,
) -> None:
    app_path = _write_static_workspace(tmp_path)

    result = runner.invoke(
        cli.app,
        ["generate", str(tmp_path), "--subdir=apps/site"],
    )

    assert result.exit_code == 0, result.output
    shipit_file = _subdir_shipit_file(tmp_path)
    shipit = shipit_file.read_text()
    assert "build = staticfile_build(config)" in shipit
    assert "staticfile_serve(config, build)" in shipit
    assert 'app_subdir = "apps/site"' in shipit

    base_config = Config()
    provider_cls = load_provider(app_path, base_config)
    provider_config = load_provider_config(provider_cls, app_path, base_config)
    provider_config.app_subdir = "apps/site"
    build_backend = LocalBuildBackend(tmp_path, tmp_path / "assets")
    local_runner = LocalRunner(build_backend, tmp_path)

    _ctx, serve = evaluate_shipit(
        shipit_file, build_backend, local_runner, provider_config
    )

    # The site is copied from the subdir into the flat static_app mount.
    assert isinstance(serve.build[0], WorkdirStep)
    assert serve.build[0].path == build_backend.get_build_mount_path("static_app")
    copy_step = serve.build[1]
    assert isinstance(copy_step, CopyStep)
    assert copy_step.source == "apps/site/public"
    assert copy_step.target == "."
    assert copy_step.ignore == [".git"]
    assert [mount.name for mount in (serve.mounts or [])] == ["static_app"]


def test_generate_subdir_rewrites_active_build_mount_paths(
    tmp_path: Path,
) -> None:
    _write_go_workspace(tmp_path)

    result = runner.invoke(
        cli.app,
        ["generate", str(tmp_path), "--subdir=apps/site"],
    )

    assert result.exit_code == 0, result.output

    build_backend, _ctx, serve, config = _evaluate_subdir_shipit(
        tmp_path, tmp_path / "apps" / "site"
    )
    temp_path = build_backend.get_build_mount_path("temp")
    app_path = build_backend.get_build_mount_path("app")

    assert any(
        isinstance(step, CopyStep) and step.source == "." and step.ignore == [".git"]
        for step in serve.build
    )
    # The Go build runs inside the subdir of the temp mount...
    env_step = next(step for step in serve.build if isinstance(step, EnvStep))
    assert env_step.variables.get("GOPATH") == f"{temp_path}/apps/site"
    # ...and only the binary is copied into the flat app mount.
    assert (
        f"cp {config.serve_binary} {app_path}/{config.serve_binary}"
        in _run_commands(serve)
    )
    assert serve.commands["start"].endswith(f"/app/{config.serve_binary}")


def test_generate_python_subdir_uses_temp_build_mount(
    tmp_path: Path,
) -> None:
    app_path = _write_python_workspace(tmp_path)

    result = runner.invoke(
        cli.app,
        ["generate", str(tmp_path), "--subdir=apps/site"],
    )

    assert result.exit_code == 0, result.output
    shipit_file = _subdir_shipit_file(tmp_path)
    shipit = shipit_file.read_text()
    assert "build = python_build(config)" in shipit
    assert "python_serve(config, build)" in shipit
    assert 'app_subdir = "apps/site"' in shipit

    base_config = Config()
    provider_cls = load_provider(app_path, base_config)
    provider_config = load_provider_config(provider_cls, app_path, base_config)
    provider_config.app_subdir = "apps/site"
    build_backend = LocalBuildBackend(tmp_path, tmp_path / "assets")
    local_runner = LocalRunner(build_backend, tmp_path)

    _ctx, serve = evaluate_shipit(
        shipit_file, build_backend, local_runner, provider_config
    )

    # Build stages in the temp mount and enters the subdir...
    temp_path = build_backend.get_build_mount_path("temp")
    workdirs = [
        step.path for step in serve.build if isinstance(step, WorkdirStep)
    ]
    assert workdirs == [temp_path, temp_path / "apps" / "site"]
    # ...installs without narrowing inputs (subdir builds need all files)...
    install_step = next(
        step
        for step in serve.build
        if isinstance(step, RunStep)
        and step.command.startswith("uv add -r requirements.txt")
    )
    assert install_step.inputs is None
    # ...and copies the built app into the flat app mount that is served.
    app_mount_path = build_backend.get_build_mount_path("app")
    assert any(
        isinstance(step, RunStep) and step.command == f"cp -R . {app_mount_path}"
        for step in serve.build
    )
    assert serve.cwd
    assert serve.cwd.endswith("/app")
    assert [mount.name for mount in (serve.mounts or [])] == ["app", "venv"]


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

    build_backend_instances, runner_instances = _patch_fake_build_runner(
        monkeypatch
    )

    result = runner.invoke(
        cli.app,
        [
            "build",
            str(tmp_path),
            "--shipit-path",
            str(_subdir_shipit_file(tmp_path)),
        ],
    )

    assert result.exit_code == 0, result.output
    assert build_backend_instances[-1].src_dir == tmp_path.resolve()
    assert build_backend_instances[-1].shipit_dir == (
        tmp_path.resolve() / ".shipit" / "apps-site"
    )
    assert runner_instances[-1].shipit_dir == (
        tmp_path.resolve() / ".shipit" / "apps-site"
    )
    serve = runner_instances[-1].serve
    assert serve.cwd.endswith("/app")
    install_step = next(
        step
        for step in build_backend_instances[-1].build_args["steps"]
        if isinstance(step, RunStep) and step.group == "install"
    )
    assert install_step.inputs is None


def test_build_subdir_uses_app_specific_shipit_by_default(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _write_node_workspace(tmp_path)
    (tmp_path / "Shipit").write_text("this is not valid starlark\n")
    generate_result = runner.invoke(
        cli.app,
        ["generate", str(tmp_path), "--subdir=apps/site"],
    )
    assert generate_result.exit_code == 0, generate_result.output

    build_backend_instances, runner_instances = _patch_fake_build_runner(
        monkeypatch
    )

    result = runner.invoke(
        cli.app,
        ["build", str(tmp_path), "--subdir=apps/site"],
    )

    assert result.exit_code == 0, result.output
    assert build_backend_instances[-1].src_dir == tmp_path.resolve()
    assert build_backend_instances[-1].shipit_dir == (
        tmp_path.resolve() / ".shipit" / "apps-site"
    )
    assert runner_instances[-1].shipit_dir == (
        tmp_path.resolve() / ".shipit" / "apps-site"
    )
    assert runner_instances[-1].serve.cwd.endswith("/app")


def test_run_subdir_uses_app_specific_shipit_dir(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _write_node_workspace(tmp_path)
    build_backend_instances, runner_instances = _patch_fake_build_runner(
        monkeypatch
    )

    result = runner.invoke(
        cli.app,
        ["run", str(tmp_path), "--subdir=apps/site", "--start"],
    )

    assert result.exit_code == 0, result.output
    assert build_backend_instances[-1].shipit_dir == (
        tmp_path.resolve() / ".shipit" / "apps-site"
    )
    assert runner_instances[-1].shipit_dir == (
        tmp_path.resolve() / ".shipit" / "apps-site"
    )
    assert runner_instances[-1].calls == [
        {
            "command": "start",
            "volume_mappings": {},
            "env": {"PORT": "8080"},
        }
    ]


def test_deploy_subdir_uses_app_specific_shipit_dir(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _write_node_workspace(tmp_path)
    build_backend_instances, _runner_instances = _patch_fake_build_runner(
        monkeypatch
    )
    wasmer_runner_instances = []

    class FakeWasmerRunner:
        def __init__(
            self,
            build_backend,
            src_dir: Path,
            registry=None,
            token=None,
            bin=None,
            shipit_dir: Path | None = None,
        ) -> None:
            self.build_backend = build_backend
            self.src_dir = src_dir
            self.registry = registry
            self.token = token
            self.bin = bin
            self.shipit_dir = shipit_dir or src_dir / ".shipit"
            self.deploy_calls = []
            wasmer_runner_instances.append(self)

        def deploy_config(self, config_path: Path) -> None:
            self.deploy_config_path = config_path

        def deploy(self, app_owner=None, app_name=None) -> None:
            self.deploy_calls.append(
                {
                    "app_owner": app_owner,
                    "app_name": app_name,
                }
            )

    monkeypatch.setattr(cli, "WasmerRunner", FakeWasmerRunner)

    result = runner.invoke(
        cli.app,
        ["deploy", str(tmp_path), "--subdir=apps/site"],
    )

    assert result.exit_code == 0, result.output
    assert build_backend_instances[-1].shipit_dir == (
        tmp_path.resolve() / ".shipit" / "apps-site"
    )
    assert wasmer_runner_instances[-1].shipit_dir == (
        tmp_path.resolve() / ".shipit" / "apps-site"
    )
    assert wasmer_runner_instances[-1].deploy_calls == [
        {
            "app_owner": None,
            "app_name": None,
        }
    ]


def test_plan_accepts_subdir_and_reports_app_provider(tmp_path: Path) -> None:
    _write_node_workspace(tmp_path)

    result = runner.invoke(
        cli.app,
        ["plan", str(tmp_path), "--subdir=apps/site", "--regenerate"],
    )

    assert result.exit_code == 0, result.output
    assert _subdir_shipit_file(tmp_path).exists()
    assert not (tmp_path / "Shipit").exists()
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
        out = kwargs["out"] or cli.default_shipit_path(
            cli.resolve_project_paths(path, kwargs["subdir"])
        )
        out.write_text('app_subdir = "apps/site"\n')

    def fake_build(path: Path, **kwargs) -> None:
        calls["build"] = (path, kwargs)

    monkeypatch.setattr(cli, "generate", fake_generate)
    monkeypatch.setattr(cli, "build", fake_build)

    result = runner.invoke(cli.app, ["auto", str(tmp_path), "--subdir=apps/site"])

    assert result.exit_code == 0, result.output
    assert calls["generate"][0] == tmp_path.resolve()
    assert calls["generate"][1]["subdir"] == Path("apps/site")
    assert calls["generate"][1]["out"] is None
    assert calls["build"][0] == tmp_path.resolve()
    assert calls["build"][1]["subdir"] == Path("apps/site")
    assert calls["build"][1]["shipit_path"] is None
    assert (tmp_path / "Shipit.apps-site").exists()
    assert not (tmp_path / "Shipit").exists()


def test_auto_passes_subdir_to_run_and_deploy(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _write_node_workspace(tmp_path)
    calls = {}

    def fake_generate(path: Path, **kwargs) -> None:
        calls["generate"] = (path, kwargs)
        out = kwargs["out"] or cli.default_shipit_path(
            cli.resolve_project_paths(path, kwargs["subdir"])
        )
        out.write_text('app_subdir = "apps/site"\n')

    def fake_build(path: Path, **kwargs) -> None:
        calls["build"] = (path, kwargs)

    def fake_run(path: Path, **kwargs) -> None:
        calls["run"] = (path, kwargs)

    def fake_deploy(path: Path, **kwargs) -> None:
        calls["deploy"] = (path, kwargs)

    monkeypatch.setattr(cli, "generate", fake_generate)
    monkeypatch.setattr(cli, "build", fake_build)
    monkeypatch.setattr(cli, "run", fake_run)
    monkeypatch.setattr(cli, "deploy", fake_deploy)

    result = runner.invoke(
        cli.app,
        [
            "auto",
            str(tmp_path),
            "--subdir=apps/site",
            "--start",
            "--wasmer-deploy",
        ],
    )

    assert result.exit_code == 0, result.output
    assert calls["run"][0] == tmp_path.resolve()
    assert calls["run"][1]["subdir"] == Path("apps/site")
    assert calls["deploy"][0] == tmp_path.resolve()
    assert calls["deploy"][1]["subdir"] == Path("apps/site")


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
