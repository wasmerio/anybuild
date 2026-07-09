"""resolve_project_context: the Shipit file location must not affect probes."""

import json
from pathlib import Path

from shipit.cli import resolve_project_context
from shipit.generator import STARLARK_ENTRYPOINTS, generate_shipit_loader
from shipit.shipit_types import RunStep


def test_out_of_tree_shipit_path_probes_the_workspace(tmp_path):
    """file_exists() must target the workspace even when the Shipit file
    lives elsewhere (--shipit-path, --temp-shipit)."""
    workspace = tmp_path / "app"
    workspace.mkdir()
    (workspace / "package.json").write_text(
        json.dumps({"name": "app", "scripts": {"start": "node index.js"}})
    )
    (workspace / "package-lock.json").write_text("{}")
    (workspace / "index.js").write_text("console.log('hi')\n")

    elsewhere = tmp_path / "elsewhere"
    elsewhere.mkdir()
    shipit_file = elsewhere / "Shipit"
    shipit_file.write_text(generate_shipit_loader(STARLARK_ENTRYPOINTS["node"]))

    context = resolve_project_context(workspace, shipit_path=shipit_file)

    commands = [
        step.command
        for step in context.serve.build
        if isinstance(step, RunStep)
    ]
    assert any("npm install" in command for command in commands), commands
