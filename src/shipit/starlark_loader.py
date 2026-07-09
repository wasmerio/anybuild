"""Starlark module loading and config injection for Shipit files.

Shipit files can load provider libraries with Bazel-style labels:

    load("//shipit/tools:python.shipit", "python_build_and_serve")

Label resolution rules:
  //shipit/...:file.shipit   bundled stdlib (ships with the CLI, `starlib/`)
  //pkg/path:file.shipit     relative to the user's project root
  file.shipit / ./file       relative to the loading file

Modules are evaluated bottom-up, frozen, and served to the evaluator through
a ``DictFileLoader`` keyed by the original label strings. Library modules get
the same builtins as the entry file but *not* ``config`` — libraries receive
the config as a function parameter, which keeps them pure and composable.

The module graph is evaluated fresh on every run: the builtins are methods of
a per-run ``Ctx``, so frozen modules cannot be cached across evaluations.
"""

from pathlib import Path
from types import SimpleNamespace
from typing import Any, Dict, Optional, Tuple

import xingque as sl
from pydantic import BaseModel

STARLIB_ROOT = Path(__file__).resolve().parent / "starlib"
STARLIB_NAMESPACE = "shipit"

# Strict superset of the previous dialect: legacy fully-inlined Shipit files
# keep evaluating unchanged.
DIALECT = sl.Dialect(
    enable_def=True,
    enable_lambda=True,
    enable_load=True,
    enable_keyword_only_arguments=True,
    enable_top_level_stmt=True,
    enable_f_strings=True,
)

LIBRARY_EXTENSIONS = [
    sl.LibraryExtension.STRUCT_TYPE,
    sl.LibraryExtension.JSON,
    sl.LibraryExtension.PRINT,
    sl.LibraryExtension.PPRINT,
    sl.LibraryExtension.PARTIAL,
    sl.LibraryExtension.MAP,
    sl.LibraryExtension.FILTER,
    sl.LibraryExtension.DEBUG,
]


def globals_builder() -> "sl.GlobalsBuilder":
    return sl.GlobalsBuilder.extended_by(LIBRARY_EXTENSIONS)


def resolve_label(label: str, loading_file: Path, project_root: Path) -> Path:
    if label.startswith("//"):
        pkg, sep, fname = label[2:].partition(":")
        if not sep or not fname:
            raise ValueError(
                f"Invalid load label {label!r}: expected //package:file.shipit"
            )
        if pkg == STARLIB_NAMESPACE or pkg.startswith(STARLIB_NAMESPACE + "/"):
            rel = pkg[len(STARLIB_NAMESPACE) :].strip("/")
            return (STARLIB_ROOT / rel / fname) if rel else (STARLIB_ROOT / fname)
        return project_root / pkg / fname
    return (loading_file.parent / label).resolve()


def eval_module_graph(
    source: str,
    entry_path: Path,
    entry_globals: "sl.Globals",
    lib_globals: "sl.Globals",
    project_root: Path,
    entry_name: str = "Shipit",
) -> "sl.Module":
    """Evaluate a Shipit entry file and its load() graph."""
    # Keyed by resolved path, not label: relative labels resolve per loading
    # file, so the same label string can name different modules. The stack
    # carries (key, label) pairs — keys for cycle detection, labels for the
    # error message.
    frozen: Dict[str, "sl.FrozenModule"] = {}

    def loader_for(
        ast: "sl.AstModule", path: Path, stack: Tuple[Tuple[str, str], ...]
    ) -> Optional["sl.DictFileLoader"]:
        deps = {}
        for load_stmt in ast.loads:
            label = load_stmt.module_id
            deps[label] = load_module(
                label, resolve_label(label, path, project_root), stack
            )
        return sl.DictFileLoader(deps) if deps else None

    def load_module(
        label: str, path: Path, stack: Tuple[Tuple[str, str], ...]
    ) -> "sl.FrozenModule":
        key = str(path.resolve())
        if key in frozen:
            return frozen[key]
        if any(key == seen_key for seen_key, _ in stack):
            labels = [seen_label for _, seen_label in stack]
            raise ValueError("load() cycle: " + " -> ".join(labels + [label]))
        if not path.is_file():
            raise FileNotFoundError(
                f"load({label!r}): no module at {path}"
            )
        ast = sl.AstModule.parse(label, path.read_text(), dialect=DIALECT)
        loader = loader_for(ast, path, stack + ((key, label),))
        module = sl.Module()
        evaluator = sl.Evaluator(module)
        # Catch statically-detectable errors (e.g. wrong arity in branches
        # this evaluation never executes) at load time instead of at runtime.
        evaluator.enable_static_typechecking(True)
        if loader is not None:
            evaluator.set_loader(loader)
        evaluator.eval_module(ast, lib_globals)
        frozen[key] = module.freeze()
        return frozen[key]

    ast = sl.AstModule.parse(entry_name, source, dialect=DIALECT)
    loader = loader_for(ast, entry_path, ())
    module = sl.Module()
    evaluator = sl.Evaluator(module)
    evaluator.enable_static_typechecking(True)
    if loader is not None:
        evaluator.set_loader(loader)
    evaluator.eval_module(ast, entry_globals)
    return module


def config_view(config: BaseModel) -> SimpleNamespace:
    """Plain-data view of a provider config for Starlark.

    Enums collapse to their values, sets become sorted lists, nested models
    become attribute-accessible namespaces; dict-typed fields stay dicts.
    """
    data = config.model_dump(mode="json")
    view = _view_value(data, config)
    assert isinstance(view, SimpleNamespace)
    return view


def _view_value(dumped: Any, live: Any) -> Any:
    if isinstance(live, BaseModel) and isinstance(dumped, dict):
        return SimpleNamespace(
            **{key: _view_value(value, getattr(live, key, None)) for key, value in dumped.items()}
        )
    if isinstance(dumped, dict):
        return dict(dumped)
    if isinstance(live, (set, frozenset)):
        return sorted(dumped)
    if isinstance(dumped, list):
        return list(dumped)
    return dumped
