"""Evaluator builtins: file_exists containment and serve() list handling."""

from pathlib import Path

import pytest

from shipit.evaluator import Ctx
from shipit.shipit_types import RunStep, Service


def _ctx(source_dir=None) -> Ctx:
    # file_exists and serve() never touch the backend or runner.
    return Ctx(None, None, source_dir=source_dir)


def test_file_exists_follows_symlinks_out_of_the_tree(tmp_path):
    shared = tmp_path / "shared"
    shared.mkdir()
    project = tmp_path / "project"
    project.mkdir()
    (project / "wp-content").symlink_to(shared, target_is_directory=True)

    assert _ctx(project).file_exists("wp-content") is True


def test_file_exists_rejects_traversal(tmp_path):
    project = tmp_path / "project"
    project.mkdir()
    (tmp_path / "outside.txt").write_text("x")

    with pytest.raises(ValueError, match="escapes"):
        _ctx(project).file_exists("../outside.txt")


def test_file_exists_rejects_absolute_paths(tmp_path):
    with pytest.raises(ValueError, match="project-relative"):
        _ctx(tmp_path).file_exists(str(tmp_path / "x"))


def test_serve_drops_none_entries_from_every_list(tmp_path):
    serve = _ctx(tmp_path).serve(
        name="app",
        provider="test",
        build=[RunStep("echo hi"), None],
        deps=[],
        commands={"start": "run"},
        services=[Service("db", "mysql"), None],
    )
    assert serve.build == [RunStep("echo hi")]
    assert serve.services == [Service("db", "mysql")]
