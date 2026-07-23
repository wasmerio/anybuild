from pathlib import Path

import pytest

from shipit.generator import load_provider
from shipit.providers.base import Config
from shipit.providers.node import NodeProvider
from shipit.providers.staticfile import (
    StaticFileProvider,
    compute_redirects_config,
)


def test_staticfile_redirects_generate_sws_config_from_static_dir(tmp_path) -> None:
    static_dir = tmp_path / "site"
    static_dir.mkdir()
    (static_dir / "_redirects").write_text(
        "/docs/* /guides/:splat/ 301\n"
        "/blog/:slug /posts/:slug 302\n"
    )

    assert compute_redirects_config(tmp_path, "site") == (
        "[[advanced.redirects]]\n"
        'source = "/docs/{**}"\n'
        'destination = "/guides/$1/"\n'
        "kind = 301\n"
        "\n"
        "[[advanced.redirects]]\n"
        'source = "/blog/{*}"\n'
        'destination = "/posts/$1"\n'
        "kind = 302\n"
    )


def test_staticfile_redirects_fall_back_to_project_root(tmp_path) -> None:
    (tmp_path / "site").mkdir()
    (tmp_path / "_redirects").write_text("/docs/* /guides/:splat/ 301\n")

    assert compute_redirects_config(tmp_path, "site") == (
        "[[advanced.redirects]]\n"
        'source = "/docs/{**}"\n'
        'destination = "/guides/$1/"\n'
        "kind = 301\n"
    )


def test_staticfile_redirects_reject_unsupported_conditions(tmp_path) -> None:
    redirects_path = tmp_path / "_redirects"
    redirects_path.write_text("/docs/* /guides/:splat/ 301 Country=us\n")

    with pytest.raises(
        ValueError, match="conditions and forced redirects are not supported"
    ):
        compute_redirects_config(tmp_path, None)


def test_staticfile_detects_unbuilt_node_project_with_root_index(
    tmp_path: Path,
    capsys,
) -> None:
    (tmp_path / "index.html").write_text("<h1>Static app</h1>\n")
    (tmp_path / "package.json").write_text(
        """{
  "scripts": {
    "copy:web": "node scripts/copy-web.mjs",
    "build:apk": "npm run copy:web && ./gradlew assembleDebug"
  },
  "dependencies": {
    "@capacitor/core": "^6.2.1"
  }
}
"""
    )

    result = StaticFileProvider.detect(tmp_path, Config())

    assert result is not None
    assert result.score == 15
    assert load_provider(tmp_path, Config()) is StaticFileProvider
    assert "Warning:" not in capsys.readouterr().err


@pytest.mark.parametrize(
    "package_json",
    [
        '{"scripts": {"start": "node server.js"}}\n',
        '{"scripts": {"build": "node build.js"}}\n',
        '{"dependencies": {"express": "^5.0.0"}}\n',
    ],
)
def test_staticfile_does_not_claim_node_project_with_runtime_or_build(
    tmp_path: Path,
    package_json: str,
) -> None:
    (tmp_path / "index.html").write_text("<h1>Fallback page</h1>\n")
    (tmp_path / "package.json").write_text(package_json)

    assert StaticFileProvider.detect(tmp_path, Config()) is None
    assert load_provider(tmp_path, Config()) is NodeProvider
