"""User command overrides are applied by the Starlark stdlib.

The stdlib serve() assembler honors config.commands.* inside evaluation, and the CLI's
legacy post-eval pass (kept for old fully-inlined Shipit files) re-applies
the same overrides — these tests pin that the combination stays idempotent:
each override lands exactly once.
"""

from pathlib import Path

from shipit.shipit_types import RunStep

from plan_helpers import evaluate_project_plan


def _python_workspace(tmp_path: Path) -> Path:
    workspace = tmp_path / "app"
    workspace.mkdir()
    (workspace / "requirements.txt").write_text("flask==3.0.0\n")
    (workspace / "main.py").write_text("print('ok')\n")
    return workspace


def test_build_and_install_overrides_replace_group_steps(tmp_path: Path) -> None:
    workspace = _python_workspace(tmp_path)

    _backend, _ctx, serve, _config = evaluate_project_plan(
        workspace,
        tmp_path,
        config_overrides={
            "commands": {
                "install": "make install",
                "build": "make assets",
                "start": "make serve --port $PORT",
            }
        },
    )

    install_steps = [
        step
        for step in serve.build
        if isinstance(step, RunStep) and step.group == "install"
    ]
    build_steps = [
        step
        for step in serve.build
        if isinstance(step, RunStep) and step.group == "build"
    ]
    # The FIRST step of each group is replaced (later same-group steps are
    # kept — long-standing CLI semantics) and nothing is applied twice.
    assert [step.command for step in install_steps] == [
        "make install",
        "uv add -r requirements.txt uvicorn",
    ]
    assert [step.command for step in build_steps] == ["make assets"]
    # start override wins and $PORT is substituted.
    assert serve.commands["start"] == "make serve --port 8080"


def test_build_override_appends_when_no_group_step_exists(tmp_path: Path) -> None:
    workspace = tmp_path / "site"
    public = workspace / "public"
    public.mkdir(parents=True)
    (public / "index.html").write_text("<h1>ok</h1>\n")

    _backend, _ctx, serve, _config = evaluate_project_plan(
        workspace,
        tmp_path,
        config_overrides={"commands": {"build": "make site"}},
    )

    build_steps = [
        step
        for step in serve.build
        if isinstance(step, RunStep) and step.group == "build"
    ]
    # Static builds have no build-group step; the override is appended once.
    assert [step.command for step in build_steps] == ["make site"]
    assert serve.build[-1] is build_steps[0]
