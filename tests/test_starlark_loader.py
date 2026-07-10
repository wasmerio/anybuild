"""Module-graph loading: relative-label resolution, caching, and cycles."""

from pathlib import Path

import pytest

from shipit.starlark_loader import eval_module_graph, globals_builder


def _eval(entry: Path, project_root: Path):
    glb = globals_builder().build()
    return eval_module_graph(
        source=entry.read_text(),
        entry_path=entry,
        entry_globals=glb,
        lib_globals=glb,
        project_root=project_root,
    )


def _write_tree(root: Path, files: dict) -> None:
    for rel, text in files.items():
        target = root / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)


def test_same_relative_label_resolves_per_loading_file(tmp_path):
    """Two files loading the same relative label get their own neighbours."""
    _write_tree(
        tmp_path,
        {
            "Shipit": (
                'load("util.shipit", "WHO")\n'
                'load("lib/helpers.shipit", "HELPER_WHO")\n'
                "ROOT_WHO = WHO\n"
                "LIB_WHO = HELPER_WHO\n"
            ),
            "util.shipit": 'WHO = "root-util"\n',
            "lib/helpers.shipit": (
                'load("util.shipit", "WHO")\nHELPER_WHO = WHO\n'
            ),
            "lib/util.shipit": 'WHO = "lib-util"\n',
        },
    )
    module = _eval(tmp_path / "Shipit", tmp_path)
    assert module.get("ROOT_WHO") == "root-util"
    assert module.get("LIB_WHO") == "lib-util"


def test_acyclic_reuse_of_a_label_is_not_a_cycle(tmp_path):
    """A repeated label along one load chain is fine when the files differ."""
    _write_tree(
        tmp_path,
        {
            "Shipit": 'load("util.shipit", "WHO")\nROOT_WHO = WHO\n',
            "util.shipit": (
                'load("lib/helpers.shipit", "HELPER_WHO")\n'
                'WHO = "root:" + HELPER_WHO\n'
            ),
            "lib/helpers.shipit": (
                'load("util.shipit", "WHO")\nHELPER_WHO = WHO\n'
            ),
            "lib/util.shipit": 'WHO = "lib-util"\n',
        },
    )
    module = _eval(tmp_path / "Shipit", tmp_path)
    assert module.get("ROOT_WHO") == "root:lib-util"


def test_true_cycle_is_detected(tmp_path):
    _write_tree(
        tmp_path,
        {
            "Shipit": 'load("a.shipit", "A")\nX = A\n',
            "a.shipit": 'load("b.shipit", "B")\nA = B\n',
            "b.shipit": 'load("a.shipit", "A")\nB = A\n',
        },
    )
    with pytest.raises(ValueError, match="load\\(\\) cycle"):
        _eval(tmp_path / "Shipit", tmp_path)
