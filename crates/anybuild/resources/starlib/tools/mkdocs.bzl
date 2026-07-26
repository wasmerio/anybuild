"""MkDocs sites: install with uv (python build), build the site, serve static.

The first real composition in the stdlib: a python build fragment feeding a
staticfile serve — what used to be a Python provider class gluing two other
providers together through emitted source strings.
"""

load("//anybuild:serve.bzl", "build")
load("//anybuild/tools:python.bzl", "python_build")
load("//anybuild/tools:staticfile.bzl", "staticfile_serve", "sws_config_mount")

def mkdocs_config(schema = 1, **kwargs):
    return config(provider = "mkdocs", schema = schema, **kwargs)

def mkdocs_build(config, static_app = None):
    """Install mkdocs with uv, then build the site into static_app."""
    py = python_build(config, serving = False)
    static_app = static_app or mount("static_app")
    static_config = sws_config_mount(config)

    return build(
        steps = py.steps + [
            run("uv run mkdocs build --site-dir={}".format(static_app.path), outputs = ["."], group = "build"),
        ],
        serve_deps = py.serve_deps,
        mounts = [static_app] + ([static_config] if static_config != None else []),
        static_app = static_app,
        static_config = static_config,
    )
