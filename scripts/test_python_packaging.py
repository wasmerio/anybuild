"""Offline packaging tests (requires Hatch, pip and a built Anybuild binary)."""

from importlib.metadata import distributions
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
sys.dont_write_bytecode = True
ANYBUILD = os.environ.get("ANYBUILD_BIN", str(ROOT / "target/debug/anybuild"))


class CrossRequirementsTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="anybuild packaging ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.wheels = self.root / "wheels"
        self.wheels.mkdir()
        (self.root / "main.py").write_text("print('hello')\n")

    def wheel(self, name, version, dependencies=(), extras=(), native=False):
        normalized = name.replace("-", "_")
        tag = "cp313-cp313-wasix_wasm32" if native else "py3-none-any"
        metadata = f"Metadata-Version: 2.1\nName: {name}\nVersion: {version}\n"
        metadata += "".join(f"Provides-Extra: {x}\n" for x in extras)
        metadata += "".join(f"Requires-Dist: {x}\n" for x in dependencies)
        info = f"{normalized}-{version}.dist-info"
        path = self.wheels / f"{normalized}-{version}-{tag}.whl"
        with zipfile.ZipFile(path, "w") as archive:
            archive.writestr(info + "/METADATA", metadata)
            archive.writestr(
                info + "/WHEEL",
                f"Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: {tag}\n",
            )
            archive.writestr(info + "/RECORD", "")

    def requirements(self, dependencies, extra=""):
        manifest = self.root / "pyproject.toml"
        manifest.write_text(
            '[project]\nname = "app"\nversion = "1.0"\ndependencies = '
            + json.dumps(dependencies) + "\n" + extra
        )
        return manifest

    def plan(self, success=True, **config):
        result = subprocess.run(
            [ANYBUILD, "plan", str(self.root), "--provider", "python",
             "--config", json.dumps({"python_cross_platform": "wasix_wasm32", **config})],
            text=True, capture_output=True,
        )
        if not success:
            self.assertNotEqual(result.returncode, 0)
            return result.stderr
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)["config"]

    def install(self, requirements, success=True, **config):
        planned = self.plan(**config)
        requirement_input = None
        if requirements.name == "pyproject.toml":
            exported = subprocess.run(
                [sys.executable, "-m", "hatch", "dep", "show", "requirements",
                 "--project-only"],
                cwd=self.root, text=True, capture_output=True,
            )
            if exported.returncode:
                self.assertFalse(success, exported.stderr)
                return exported.stdout + exported.stderr
            arguments = ["-r", "/dev/stdin"]
            requirement_input = exported.stdout
        else:
            arguments = ["-r", str(requirements)]
        arguments += planned.get("python_extra_dependencies", [])
        result = subprocess.run(
            [sys.executable, "-m", "pip", "install", "--no-index",
             "--disable-pip-version-check", "--no-cache-dir",
             "--find-links", str(self.wheels), "--only-binary=:all:",
             "--platform", "wasix_wasm32", "--python-version", "3.13",
             "--target", str(self.root / "target"), "--ignore-installed",
             *arguments],
            cwd=self.root, text=True, capture_output=True, input=requirement_input,
        )
        if not success:
            self.assertNotEqual(result.returncode, 0)
            return result.stdout + result.stderr
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return {
            package.metadata["Name"]: package.version
            for package in distributions(path=[str(self.root / "target")])
        }

    def parent_wheels(self):
        self.wheel("parent", "2.0", ["core==2.0"])
        self.wheel("parent", "1.0", ["core==1.0"])
        self.wheel("core", "1.0", native=True)

    def test_target_resolver_backtracks_to_available_native_wheel(self):
        self.parent_wheels()
        resolved = self.install(self.requirements(["parent>=1"]))
        self.assertEqual(resolved, {"parent": "1.0", "core": "1.0"})

    def test_explicit_pin_is_not_silently_relaxed(self):
        self.parent_wheels()
        error = self.install(self.requirements(["parent==2.0"]), success=False)
        self.assertIn("core==2.0", error)

    def test_extras_and_markers_survive_into_installed_dependencies(self):
        self.wheel("db", "1.0", [
            'db-binary==1.0; extra == "binary"',
            'db-pool==1.0; extra == "pool"',
        ], ["binary", "pool"])
        self.wheel("db-binary", "1.0", native=True)
        self.wheel("db-pool", "1.0")
        requirement = 'db[binary,pool]>=1; python_version >= "3.10"'
        path = self.requirements([requirement])
        self.assertEqual(set(self.install(path)), {"db", "db-binary", "db-pool"})

    def test_requirements_includes_and_constraints(self):
        self.parent_wheels()
        (self.root / "deps.txt").write_text("parent>=1\n")
        (self.root / "constraints.txt").write_text("parent<2\n")
        path = self.root / "requirements.txt"
        path.write_text("-r deps.txt\n-c constraints.txt\n")
        self.assertEqual(self.install(path)["parent"], "1.0")

    def test_uv_constraints_are_ignored_for_target_resolution(self):
        self.wheel("parent", "1.0")
        self.wheel("parent", "2.0")
        path = self.requirements(
            ["parent>=1"], '[tool.uv]\nconstraint-dependencies = ["parent<2"]\n'
        )
        self.assertEqual(self.install(path)["parent"], "2.0")

    def test_source_overrides_fail_instead_of_installing_from_wrong_index(self):
        for override in [
            '[tool.uv.sources]\nprivate-package = {path = "../private"}\n',
            '[tool.uv]\noverride-dependencies = ["private-package==1"]\n',
        ]:
            self.requirements(
                ["private-package"], override,
            )
            self.assertIn("source/dependency overrides", self.plan(success=False))
            self.plan(python_cross_platform=None)

    def test_constraints_do_not_install_unneeded_packages(self):
        self.wheel("parent", "1.0")
        manifest = self.requirements(
            ["parent"], '[tool.uv]\nconstraint-dependencies = ["unused==1"]\n'
        )
        self.assertEqual(self.install(manifest), {"parent": "1.0"})

    def test_extra_dependencies_resolve_together_with_project_dependencies(self):
        self.parent_wheels()
        self.wheel("server", "1.0", ["core==1.0"])
        manifest = self.requirements(["parent>=1"])
        self.assertEqual(
            self.install(manifest, python_extra_dependencies=["server>=1"]),
            {"parent": "1.0", "core": "1.0", "server": "1.0"},
        )

    def test_generated_config_does_not_freeze_project_requirements(self):
        self.wheel("parent", "1.0")
        self.wheel("parent", "2.0")
        manifest = self.requirements(["parent>=1"])
        self.assertEqual(self.install(manifest)["parent"], "2.0")
        generated = (self.root / "Anybuild").read_text()
        self.assertNotIn("python_project_requirements", generated)
        self.assertNotIn("parent>=1", generated)
        shutil.rmtree(self.root / "target")
        self.requirements(["parent<2"])
        self.assertEqual(self.install(manifest)["parent"], "1.0")
        self.assertEqual((self.root / "Anybuild").read_text(), generated)

    def test_static_dependencies_do_not_invoke_build_backend(self):
        self.wheel("parent", "1.0")
        manifest = self.requirements(
            ["parent"], '[build-system]\nrequires = []\n'
            'build-backend = "unavailable_backend"\n',
        )
        self.assertEqual(self.install(manifest), {"parent": "1.0"})

    def test_empty_project_has_no_target_dependencies(self):
        self.assertEqual(self.install(self.requirements([])), {})

    def test_dynamic_metadata_failure_is_not_silently_ignored(self):
        manifest = self.root / "pyproject.toml"
        manifest.write_text(
            '[project]\nname = "app"\nversion = "1.0"\n'
            'dynamic = ["dependencies"]\n[build-system]\nrequires = []\n'
            'build-backend = "unavailable_backend"\n'
        )
        self.assertIn("unavailable_backend", self.install(manifest, success=False))


if __name__ == "__main__":
    unittest.main()
