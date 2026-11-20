#!/usr/bin/env python3
"""Validate a release and select crates still missing from crates.io."""

import json
import subprocess
import sys
import urllib.error
import urllib.request


CRATES = ("anybuild", "anybuild-cli")


def registry_version(crate, version):
    request = urllib.request.Request(
        f"https://crates.io/api/v1/crates/{crate}/{version}",
        headers={
            "User-Agent": "anybuild-release (github.com/wasmerio/anybuild)"
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response)["version"]
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return None
        raise


def unpublished_crates(metadata, version, lookup=registry_version):
    members = set(metadata["workspace_members"])
    packages = {
        package["name"]: package
        for package in metadata["packages"]
        if package["id"] in members
    }
    for name in ("anybuild-workspace", *CRATES):
        if packages[name]["version"] != version:
            raise ValueError(f"{name} must have release version {version}")

    sdk = next(
        dependency
        for dependency in packages["anybuild-cli"]["dependencies"]
        if dependency["name"] == "anybuild"
    )
    if sdk["req"] != f"^{version}":
        raise ValueError(f"anybuild-cli must depend on anybuild {version}")

    pending = []
    for crate in CRATES:
        published = lookup(crate, version)
        if published is None:
            pending.append(crate)
        elif published["num"] != version or published["yanked"]:
            raise ValueError(f"{crate} {version} is unavailable or yanked")
    return pending


def main():
    metadata = json.loads(
        subprocess.check_output(
            [
                "cargo", "metadata", "--locked", "--no-deps",
                "--format-version", "1",
            ],
            text=True,
        )
    )
    pending = unpublished_crates(metadata, sys.argv[1])
    print(f"packages={' '.join(pending)}")


if __name__ == "__main__":
    main()
