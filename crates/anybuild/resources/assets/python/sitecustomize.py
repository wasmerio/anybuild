"""Enable WASIX compatibility, then run the application's sitecustomize."""

from importlib.machinery import EXTENSION_SUFFIXES, ExtensionFileLoader, PathFinder
from importlib.util import module_from_spec, spec_from_file_location
import os
import sys

LEGACY_SUFFIX = ".cpython-313-wasm32-wasi-threads.so"


class LegacyWasixExtensionFinder:
    def find_spec(self, fullname, path=None, target=None):
        filename = fullname.rpartition(".")[2] + LEGACY_SUFFIX
        for directory in sys.path if path is None else path:
            if not isinstance(directory, str):
                continue
            candidate = os.path.join(directory, filename)
            if os.path.isfile(candidate):
                return spec_from_file_location(
                    fullname, candidate,
                    loader=ExtensionFileLoader(fullname, candidate),
                )
        return None


def install():
    if (
        sys.implementation.name != "cpython"
        or sys.version_info[:2] != (3, 13)
        or ".cpython-313-wasm32-wasi.so" not in EXTENSION_SUFFIXES
        or LEGACY_SUFFIX in EXTENSION_SUFFIXES
    ):
        return
    # Normal importers retain priority, including for current and abi3 wheels.
    if not any(isinstance(f, LegacyWasixExtensionFinder) for f in sys.meta_path):
        sys.meta_path.append(LegacyWasixExtensionFinder())


if __name__ == "sitecustomize":
    install()

    # Keep our hook separate, then run the application's original startup hook.
    bootstrap_dir = os.path.realpath(os.path.dirname(__file__))
    search_path = [
        path for path in sys.path
        if isinstance(path, str) and os.path.realpath(path) != bootstrap_dir
    ]
    spec = PathFinder.find_spec(__name__, search_path)
    if spec is not None and spec.loader is not None:
        module = module_from_spec(spec)
        sys.modules[__name__] = module
        spec.loader.exec_module(module)
