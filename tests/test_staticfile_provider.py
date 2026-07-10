import pytest

from shipit.providers.staticfile import compute_redirects_config


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
