import unittest
import urllib.error
from unittest.mock import patch

from crate_release import CRATES, registry_version, unpublished_crates


def workspace(version="0.34.0"):
    names = ("anybuild-workspace", *CRATES)
    return {
        "workspace_members": list(names),
        "packages": [
            {
                "id": name,
                "name": name,
                "version": version,
                "dependencies": [{"name": "anybuild", "req": f"^{version}"}],
            }
            for name in names
        ],
    }


class CrateReleaseTests(unittest.TestCase):
    def test_first_publish_orders_sdk_before_cli(self):
        self.assertEqual(
            unpublished_crates(workspace(), "0.34.0", lambda *_: None),
            ["anybuild", "anybuild-cli"],
        )

    def test_retry_after_sdk_publication_only_publishes_cli(self):
        def lookup(crate, version):
            if crate == "anybuild":
                return {"num": version, "yanked": False}
            return None

        self.assertEqual(
            unpublished_crates(workspace(), "0.34.0", lookup), ["anybuild-cli"]
        )

    def test_completed_release_is_a_noop(self):
        self.assertEqual(
            unpublished_crates(
                workspace(),
                "0.34.0",
                lambda _, version: {"num": version, "yanked": False},
            ),
            [],
        )

    def test_version_mismatch_fails_before_registry_access(self):
        metadata = workspace()
        for package in metadata["packages"]:
            with self.subTest(package=package["name"]):
                package["version"] = "0.33.3"
                with self.assertRaises(ValueError):
                    unpublished_crates(metadata, "0.34.0", self.fail)
                package["version"] = "0.34.0"

    def test_stale_sdk_dependency_fails_before_registry_access(self):
        metadata = workspace()
        metadata["packages"][-1]["dependencies"][0]["req"] = "^0.33.3"
        with self.assertRaises(ValueError):
            unpublished_crates(metadata, "0.34.0", self.fail)

    def test_yanked_version_fails(self):
        with self.assertRaises(ValueError):
            unpublished_crates(
                workspace(),
                "0.34.0",
                lambda _, version: {"num": version, "yanked": True},
            )

    def test_registry_404_means_unpublished(self):
        error = urllib.error.HTTPError("url", 404, "not found", {}, None)
        with patch("urllib.request.urlopen", side_effect=error):
            self.assertIsNone(registry_version("anybuild", "0.34.0"))

    def test_registry_failure_never_counts_as_unpublished(self):
        for status in (403, 429, 500, 503):
            with self.subTest(status=status):
                error = urllib.error.HTTPError(
                    "url", status, "failed", {}, None
                )
                with patch("urllib.request.urlopen", side_effect=error):
                    with self.assertRaises(urllib.error.HTTPError):
                        registry_version("anybuild", "0.34.0")


if __name__ == "__main__":
    unittest.main()
