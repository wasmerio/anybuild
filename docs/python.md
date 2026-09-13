# Python packaging and MCP

## WASIX dependency resolution

Wasmer builds resolve runtime dependencies against PyPI and the configured
WASIX wheel index. They use the original `[project].dependencies` or
`requirements.txt`, including extras such as `psycopg[binary,pool]`, rather
than pinning the result of a host-platform resolve first. The resolver can
therefore select an older compatible parent when its newest release needs
a native wheel that is not published for WASIX yet.

For `pyproject.toml`, Anybuild runs Hatch 1.18.0 through `uvx` and pipes
`hatch dep show requirements --project-only` directly into
`uvx pip install -r /dev/stdin`. Hatch exports the dependency declarations;
pip resolves them for WASIX together with Anybuild's extra dependencies.
This runs before the host install can change the staged manifest. No custom
requirements parser, generated requirements file, or dependency snapshot in
the `Anybuild` configuration is needed. An export failure fails the build.

Explicit version bounds and pins still apply. A requirement such as
`pydantic==2.13.5` fails if its required native wheel is unavailable; Anybuild
does not relax that pin. The host environment continues to use `uv.lock`
when present. Target versions may differ from that host lock.

The default Wasmer Python package, including Python 3.13, is
`python/python@=3.13.20`. Some published CPython 3.13 WASIX wheels still use
the old `.cpython-313-wasm32-wasi-threads.so` extension filename. A temporary
[fallback importer][wasix-importer] loads these files through Python's
standard extension loader after normal import lookup fails. Current
extensions and `abi3` wheels use normal import lookup.

The single `sitecustomize.py` file is added to the serving environment only
when `python_fix_wasix_imports` is true. Wasmer defaults this setting to true
when it is unset; other runners leave it disabled. To opt out in an
`Anybuild` file, set:

```python
config = python_config(
    python_fix_wasix_imports = False,
    # Other generated settings...
)
```

The equivalent environment setting is
`ANYBUILD_PYTHON_FIX_WASIX_IMPORTS=false`. An explicit true or false overrides
the runner default. The importer itself activates only on WASIX CPython
3.13. It lives in a separate directory and chains any application
`sitecustomize.py`. Installed wheel files and `RECORD` metadata remain
untouched; no CFFI constraint is added. This handles the legacy filename,
not arbitrary binary incompatibility. The fallback can be removed once
supported wheels all use the current suffix.

[wasix-importer]: ../crates/anybuild/resources/assets/python/sitecustomize.py

Requirement-file includes (`-r`) and constraints (`-c`) are passed to pip.
For pyproject-based builds, `[tool.uv].constraint-dependencies` is ignored
by target resolution; the host install still uses uv's configuration.
uv source/dependency overrides are unsupported for target resolution and
fail with an explanation. Hatch reads static dependencies without invoking
the build backend. Dynamic metadata requires a working backend and its
source files; set `python_install_requires_all_files = True` when needed.

Installing a target wheel verifies dependency compatibility, not every
native feature it exposes. In particular, installing `psycopg[binary]`
does not by itself verify a database connection.

## Source files

Python source copies respect `.gitignore` with both local and Docker
builders, including nested rules and negated patterns. This also applies to
subdirectory projects and `python_install_requires_all_files = True`.
The usual `.git`, `.venv`, and `__pycache__` exclusions still apply.

If a build needs ignored source files, opt out explicitly:

```python
config = python_config(
    python_copy_gitignore = False,
    # Other generated settings...
)
```

The equivalent environment setting is `ANYBUILD_PYTHON_COPY_GITIGNORE=false`.
Dependency manifests listed as installation inputs are copied explicitly.

## MCP SDK 1 and SDK 2

The examples preserve both APIs:

| Example | SDK | Transport |
| --- | --- | --- |
| [python-mcp-v1](../examples/python-mcp-v1) | FastMCP 1.x | Streamable HTTP |
| [python-mcp-chatgpt-v1](../examples/python-mcp-chatgpt-v1) | FastMCP 1.x | SSE |
| [python-mcp](../examples/python-mcp) | MCPServer 2.x | Streamable HTTP |
| [python-mcp-chatgpt](../examples/python-mcp-chatgpt) | MCPServer 2.x | Streamable HTTP |

Anybuild supplies `HOST=0.0.0.0` and `PORT` for MCP applications, alongside
the `FASTMCP_HOST` and `FASTMCP_PORT` aliases used by SDK 1. A self-running
SDK 2 application must pass these values to `MCPServer.run()` explicitly;
SDK 2 does not read the FastMCP environment variables. The new examples do
this and use stateless HTTP for the included tools.

For an application that only defines a server named `mcp`, `server`, or
`app`, Anybuild supplies a launcher supporting either SDK generation. It
uses Streamable HTTP and the configured host and port. A custom start
command remains available for applications needing a different transport
or startup procedure.

The E2E suite initializes an MCP client, lists tools, and calls them on a
non-default port for each example. It also tests applications launched by
Anybuild directly, with both SDK generations.
