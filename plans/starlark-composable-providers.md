# Proposal: Composable Starlark providers (the Shipit stdlib)

## Summary

Today, providers are Python classes that **emit Starlark source as strings**, and
`generate_shipit()` stitches those strings into a fully-inlined `Shipit` file.
This proposal moves all plan construction into real Starlark libraries
(`.shipit` files) bundled with the CLI, so the generated file collapses to:

```python
load("//shipit/tools:python.shipit", "python_build_and_serve")

python_build_and_serve(config)
```

Python keeps what it is good at — filesystem detection and building a fully
resolved, statically-typed `config` — and Starlark does what it is good at —
declaratively composing the plan. Provider logic becomes ordinary functions
that call each other (`wordpress_serve` → `php_serve` → `app_serve` → `serve`),
which makes providers composable for us *and* overridable for users, without
string templating.

The first stage (static config + generated file referencing `config.*`) already
landed; this is the second stage it was designed to enable.

**Feasibility is verified**: a spike against `xingque` 0.2.1 (the engine we
already ship) confirms `load()`, module freezing via `DictFileLoader`,
host-builtin calls from library functions, and plain-data config access all
work. Details in [Engine wiring](#engine-wiring-xingque).

## Implementation status (branch `starlark-providers`)

**All providers are ported.** The stdlib now covers python, staticfile, hugo,
mkdocs, jekyll, go, php, wordpress, laravel, node, and node-static — every
provider in the registry generates the two-line loader form. The legacy
inline generator (`generate_shipit_inline`) and the provider emitter methods
remain in-tree solely as the reference implementation for the
plan-equivalence tests; deleting them is the next phase, once this has baked
in CI/e2e. The shipped code supersedes the sketches below where they differ:

- Engine plumbing: `src/shipit/starlark_loader.py` (label resolution, module
  graph eval, `config_view`), extended dialect/globals wired into
  `evaluate_shipit`. Legacy inlined Shipit files still evaluate unchanged.
- Stdlib: `src/shipit/starlib/` — `prelude.shipit`, `serve.shipit`, and
  `tools/{python,staticfile,hugo,mkdocs}.shipit`.
- **Convention refinement**: providers split into `X_build(config, ...)` (a
  build fragment returning a `build_struct(steps, serve_deps, mounts, env,
  **extra)`), `X_serve(config, build, ...)` (serve wiring around any build
  struct), and `X_build_and_serve(config, ...)` (the canonical entrypoint the
  generated file calls). The generic assembler `build_and_serve()` lives in
  `//shipit:serve.shipit` and defines the uniform override surface
  (`build_pre`, `build_post`, `extra_deps`, `extra_env`, `extra_commands`).
  This split is what providers-that-inherit compose on: mkdocs =
  `python_build` + `staticfile_serve`.
- Generator dispatch: `STARLARK_ENTRYPOINTS` in `generator.py` (keyed by
  provider name — deliberately not a class attribute, so subclasses like
  jekyll don't inherit an entrypoint they don't implement yet). Ported
  providers generate the two-liner; everything else uses the legacy inline
  generator (`generate_shipit_inline`), kept until all providers are ported.
- Config additions: base `Config.name`; `PythonConfig.{has_pyproject,
  has_requirements, has_uv_lock, install_inputs, mcp_self_running}`;
  `StaticFileConfig.redirects_config` (rendered sws.toml, computed at load).
- Verification: `tests/test_plan_equivalence.py` evaluates the legacy inline
  text and the new two-liner with the same config for **every example in the
  repo** (~80 cases: all python/static/node/nodestatic/php/wordpress/laravel/
  go/hugo/mkdocs examples, the pnpm/npm subdir workspaces, plus synthesized
  jekyll/go-subdir/python-subdir/static-subdir workspaces) and asserts
  identical normalized plans. Full non-e2e suite green; mypy clean.
- Composition patterns realized: mkdocs = `python_build` + `staticfile_serve`;
  node-static = node build fragments + `staticfile_serve`; wordpress =
  `php_build` with `build_pre`/`after_build` hooks (or an extension build into
  the wp-content mount); laravel = `php_build` with the node install/build
  hooked into `after_install`/`after_build` via `extra_use_deps`.

---

## The problem with the current state

`PythonProvider.build_steps()` is Python that writes Starlark:

```python
steps += [
    'env(UV_PROJECT_ENVIRONMENT=local_venv.path if cross_platform else venv.path, ...)',
    'copy(".", ".")' if requires_all_files and not app_subdir else None,
    f'run(f"uv sync{extra_args}"{inputs_arg}, group="install")',
]
```

This has three compounding costs:

1. **Two languages, two evaluation times.** Some conditionals run in Python at
   generation time (`if requires_all_files`), others are emitted as Starlark to
   run at eval time (`... if cross_platform else None`). Which condition lives
   where is arbitrary, and quoting/escaping (`{{PORT}}`, `\\"`) is a constant
   source of bugs.
2. **No real composition.** `MkdocsProvider` glues `PythonProvider(only_build=True)`
   and `StaticFileProvider` together by concatenating their emitted strings.
   `WordPressProvider` subclasses `PhpProvider` and threads hook lists
   (`after_install`, `after_build`) through Python method signatures.
3. **The generated file is a dead snapshot.** Users get ~50 lines of inlined
   plan. When we improve a provider, their checked-in `Shipit` doesn't benefit;
   if they edited it, regeneration clobbers their edits.

## Target architecture

```
┌─────────────────────────────────────────────────────────────────┐
│ Python (per run)                                                │
│   detect()        → pick provider                               │
│   load_config()   → probe filesystem, produce resolved config   │
│                     (ALL I/O happens here)                      │
└───────────────┬─────────────────────────────────────────────────┘
                │  config (plain data, injected as a global)
┌───────────────▼─────────────────────────────────────────────────┐
│ User project: Shipit  (generated once, user-editable, 2 lines)  │
│   load("//shipit/tools:python.shipit",                          │
│        "python_build_and_serve")                                │
│   python_build_and_serve(config)                                │
└───────────────┬─────────────────────────────────────────────────┘
                │  load()
┌───────────────▼─────────────────────────────────────────────────┐
│ Shipit stdlib (.shipit files bundled in the wheel)              │
│   tools/python.shipit    python_build_and_serve(config, ...)    │
│   tools/php.shipit       php_serve(config, **hooks)             │
│   tools/wordpress.shipit wordpress_serve → php_serve            │
│   tools/mkdocs.shipit    python_build + staticfile_serve        │
│   serve.shipit           app_serve — the higher-order serve     │
└───────────────┬─────────────────────────────────────────────────┘
                │  serve(), run(), dep(), mount(), ... (unchanged)
┌───────────────▼─────────────────────────────────────────────────┐
│ Ctx builtins (Python) → Serve plan → builders/runners           │
└─────────────────────────────────────────────────────────────────┘
```

**Design invariant:** `.shipit` files are hermetic — pure functions of
`config`. Every filesystem fact they need (does `uv.lock` exist? which
`index.php` is the docroot?) must be a config field computed by
`load_config()`. This finishes the "config is the full static snapshot" story
the first stage started.

### What stays exactly as it is

- Detection and config loading (`detect()`, `load_config()`, `--config` JSON
  merging, env-var overrides via pydantic-settings).
- The `Ctx` builtins (`serve`, `run`, `copy`, `dep`, `mount`, `volume`, `env`,
  `use`, `path`, `workdir`, `write`, `service`) and everything downstream
  (Serve plan, builders, runners, post-eval command overrides in
  `evaluate_shipit`).
- Old fully-inlined `Shipit` files in the wild — the new dialect is a strict
  superset, so they keep evaluating unchanged.

---

## The generated file, before and after

**Before** (`examples/python-fastapi/Shipit`, 50 lines):

```python
python = dep("python", config.python_version)
uv = dep("uv", config.uv_version)

app = mount("app")
venv = mount("venv")
local_venv = mount("local_venv")

python_version = config.python_version
# ... 8 more declarations ...

serve(
  name="python-fastapi",
  provider="python",
  build=[ <~15 steps> ],
  # ... deps, prepare, env, commands, mounts ...
)
```

**After**:

```python
load("//shipit/tools:python.shipit", "python_build_and_serve")

python_build_and_serve(config)
```

With a subdir, the marker line stays (keeps `read_shipit_subdir()` working):

```python
load("//shipit/tools:node.shipit", "node_build_and_serve")

app_subdir = "apps/dashboard"

node_build_and_serve(config)
```

The file is still the user's: it is real Starlark, and every knob is an
explicit keyword argument away (see [User customization](#user-customization)).

---

## Load labels and stdlib layout

Bazel-flavored labels, three resolution rules:

| Label form                          | Resolves to                                  |
|-------------------------------------|----------------------------------------------|
| `//shipit/...:file.shipit`          | bundled stdlib (`src/shipit/starlib/...`)    |
| `//path/to/pkg:file.shipit`         | user project root (project-local libraries)  |
| `helpers.shipit` / `./helpers.shipit` | relative to the loading file               |

The `//shipit/` namespace is reserved for the stdlib so project layouts can
never shadow it.

```
src/shipit/starlib/
  prelude.shipit          # merged(), compact(), config_with() — tiny helpers
  serve.shipit            # app_serve() — the higher-order serve
  tools/
    python.shipit         # python_serve(), python_build(), python_commands(), ...
    staticfile.shipit     # staticfile_serve()
    hugo.shipit           # hugo_serve()      (staticfile + hugo build)
    mkdocs.shipit         # mkdocs_serve()    (python_build + staticfile_serve)
    php.shipit            # php_serve(), php_build() with hook kwargs
    wordpress.shipit      # wordpress_serve() (php_serve + WP steps)
    node.shipit           # node_serve(), node_build()
    node_static.shipit    # nodestatic_serve() (node_build + staticfile_serve)
    go.shipit             # go_serve()
    jekyll.shipit         # jekyll_serve()
```

Starlark's rule that `load()` cannot import names starting with `_` gives us
real public/private separation for free: `python_serve` is API,
`_install_steps` is not.

---

## Engine wiring (xingque)

Everything below was validated with a working spike against xingque 0.2.1
(the version already pinned).

### Dialect

The current dialect (`enable_keyword_only_arguments`, `enable_f_strings`) grows
to:

```python
DIALECT = sl.Dialect(
    enable_def=True,            # library functions
    enable_lambda=True,
    enable_load=True,           # the whole point
    enable_keyword_only_arguments=True,
    enable_top_level_stmt=True, # allow `if` at top level of user files
    enable_f_strings=True,
)
```

Strict superset of what generated files use today ⇒ backward compatible.

**Known limitation (verified):** starlark-rust f-strings only interpolate
*simple identifiers* — `f"{config.name}"` is a parse error. Stdlib style rule:
bind to a local first (`asgi = config.asgi_application`) or use `.format()`.
This matches how generated files already behave.

### Globals

`GlobalsBuilder.standard()` becomes
`GlobalsBuilder.extended_by([STRUCT_TYPE, JSON, PRINT, PARTIAL, MAP, FILTER])`
so libraries can return `struct(...)` bundles and debug with `print()`. The
same Ctx-bound builtins are set on it as today. Library modules get these
globals but **not** `config` — libraries receive config as a parameter, which
is what makes them composable and testable. Only the entry `Shipit` file gets
`config` injected.

### Module graph loader (new `src/shipit/starlark_loader.py`, ~70 lines)

`xingque` exposes exactly the primitives needed: `AstModule.loads` (list of
`AstLoad` with `.module_id`), `Module.freeze()`, `DictFileLoader`,
`Evaluator.set_loader`.

```python
STARLIB_ROOT = Path(__file__).resolve().parent / "starlib"

def resolve_label(label: str, loading_file: Path, project_root: Path) -> Path:
    if label.startswith("//"):
        pkg, _, fname = label[2:].partition(":")
        if pkg == "shipit" or pkg.startswith("shipit/"):
            return STARLIB_ROOT / pkg[len("shipit"):].strip("/") / fname
        return project_root / pkg / fname
    return (loading_file.parent / label).resolve()

def eval_module_graph(entry: Path, globals_: sl.Globals, project_root: Path):
    frozen: dict[str, sl.FrozenModule] = {}

    def load_recursive(label: str, path: Path, stack: tuple) -> sl.FrozenModule:
        if label in frozen:
            return frozen[label]
        if label in stack:
            raise ValueError("load() cycle: " + " -> ".join(stack + (label,)))
        ast = sl.AstModule.parse(str(path), path.read_text(), dialect=DIALECT)
        deps = {
            ld.module_id: load_recursive(
                ld.module_id, resolve_label(ld.module_id, path, project_root),
                stack + (label,),
            )
            for ld in ast.loads
        }
        module = sl.Module()
        evaluator = sl.Evaluator(module)
        if deps:
            evaluator.set_loader(sl.DictFileLoader(deps))
        evaluator.eval_module(ast, globals_)
        frozen[label] = module.freeze()
        return frozen[label]

    ast = sl.AstModule.parse("Shipit", entry.read_text(), dialect=DIALECT)
    deps = {
        ld.module_id: load_recursive(
            ld.module_id, resolve_label(ld.module_id, entry, project_root), ()
        )
        for ld in ast.loads
    }
    module = sl.Module()
    evaluator = sl.Evaluator(module)
    if deps:
        evaluator.set_loader(sl.DictFileLoader(deps))
    evaluator.eval_module(ast, globals_)
    return module
```

One consequence worth being explicit about: library modules close over the
builtins they were evaluated with, and those builtins are methods of a per-run
`Ctx`. So **the module graph is evaluated fresh on every run** — no cross-run
caching of frozen modules. The stdlib is a handful of small files and
starlark-rust evaluation is sub-millisecond; if this ever matters, the future
fix is passing a context object through the call chain instead of binding it
into globals.

### Config injection

Today `glb.set("config", provider_config)` injects the pydantic object, and
attribute access works — but enum fields (`config.framework`) surface as Python
enums, which is why providers compare them in Python before emitting strings.
Starlark needs plain data, so we inject a **plain-data view**:

```python
def config_view(config: Config) -> SimpleNamespace:
    data = config.model_dump(mode="json")   # enums→values, sets→lists, Paths→str
    data["app_subdir"] = config.app_subdir  # excluded from dumps, still needed
    return _namespaces(data)                # recursive: model fields become
                                            # attr-accessible; Dict[str,str]
                                            # leaves stay dicts (keyed off the
                                            # pydantic schema, not blind recursion)
```

Now `config.framework == "django"` is a plain string comparison in Starlark,
which the enum classes already support (`PythonFramework.Django == "django"`
value-wise). The `--config` JSON override path is untouched — it merges into
the pydantic model *before* the view is built.

---

## The stdlib

Below are the real implementations. `python.shipit` is a faithful port of
today's `PythonProvider` output (checked against the golden files in
`examples/`); the others show every composition pattern the current codebase
uses. Ports are pinned exact by the [plan-equivalence tests](#verification).

### `//shipit:prelude.shipit`

```python
"""Tiny helpers shared by provider libraries."""

def merged(base, overrides):
    """Dict merge, overrides win. None-safe."""
    result = dict(base or {})
    result.update(overrides or {})
    return result

def compact(items):
    """Drop None entries (conditional steps)."""
    return [item for item in items if item != None]

def config_with(config, **overrides):
    """Copy of a config with fields replaced — for composing derived configs."""
    fields = {name: getattr(config, name) for name in dir(config)}
    fields.update(overrides)
    return struct(**fields)
```

### `//shipit:serve.shipit` — the higher-order serve

Every provider bottoms out here. It is deliberately thin: its job is to give
*every* provider the same override surface (`build_pre`, `build_post`,
`extra_env`, `extra_commands`, `extra_deps`) and to normalize the fiddly bits
(None-filtering, name defaulting, empty-collection handling) that
`generate_shipit()` does today.

```python
load("//shipit:prelude.shipit", "compact", "merged")

def app_serve(
        config,
        provider,
        build,
        name = None,
        deps = [],
        commands = {},
        env = None,
        prepare = None,
        cwd = None,
        mounts = [],
        volumes = [],
        services = [],
        # uniform user-facing override surface:
        build_pre = [],
        build_post = [],
        extra_deps = [],
        extra_env = {},
        extra_commands = {}):
    """Higher-order serve. Providers call this; users can too."""
    return serve(
        name = name or config.name,
        provider = provider,
        cwd = cwd,
        build = compact(list(build_pre) + list(build) + list(build_post)),
        deps = deps + extra_deps,
        prepare = compact(prepare) if prepare != None else None,
        env = merged(env, extra_env) if (env != None or extra_env) else None,
        commands = merged(commands, extra_commands),
        mounts = mounts,
        volumes = volumes,
        services = services,
    )
```

### `//shipit/tools:python.shipit`

Full port of `PythonProvider`. Note the shape: small pure functions
(`python_commands`, `python_env`, ...) exported individually, so both flavors
(`python_serve`, `python_build`) and downstream providers (mkdocs) reuse them —
this replaces the `only_build=True` constructor flag with an actual function.

```python
"""Python apps: uv-managed venv, optional cross-platform wheels.

Pure functions of `config`. Config fields consumed here are computed by
shipit's Python-side detection — no filesystem access happens in this file.
"""

load("//shipit:prelude.shipit", "compact", "merged")
load("//shipit:serve.shipit", "app_serve")

def python_toolchain(config):
    return struct(
        python = dep("python", config.python_version),
        uv = dep("uv", config.uv_version),
    )

def _subdir_steps(config, mount_point):
    """Build-context steps when the app lives in a subdirectory."""
    if not config.app_subdir:
        return [workdir(mount_point.path)]
    return [
        workdir(mount_point.path),
        copy(".", ".", ignore = [".git", ".venv", "__pycache__"]),
        workdir("{}/{}".format(mount_point.path, config.app_subdir)),
    ]

def _install_steps(config, venv, local_venv):
    python_version = config.python_version
    cross = config.cross_platform
    extra = " ".join(config.extra_dependencies)
    all_files = config.install_requires_all_files
    in_subdir = config.app_subdir != None
    inputs = None if (in_subdir or all_files) else config.install_inputs

    steps = []
    if config.has_pyproject:
        lock = " --locked" if config.has_uv_lock else ""
        steps += [
            env(
                UV_PROJECT_ENVIRONMENT = local_venv.path if cross else venv.path,
                UV_PYTHON_PREFERENCE = "only-system",
                UV_PYTHON = f"python{python_version}",
            ),
            copy(".", ".") if all_files and not in_subdir else None,
            run("uv sync" + lock, inputs = inputs, group = "install"),
            copy("pyproject.toml", "pyproject.toml") if not in_subdir and not all_files else None,
            run("uv add " + extra, group = "install") if extra else None,
        ]
    elif config.has_requirements or extra:
        steps += [
            env(UV_PROJECT_ENVIRONMENT = local_venv.path if cross else venv.path),
            run("uv init", inputs = [], outputs = ["uv.lock"], group = "install"),
            copy(".", ".", ignore = [".venv", ".git", "__pycache__"]) if all_files and not in_subdir else None,
        ]
        if config.has_requirements:
            steps.append(run("uv add -r requirements.txt " + extra, inputs = inputs, group = "install"))
        else:
            steps.append(run("uv add " + extra, group = "install"))
    return steps

def _cross_wheel_steps(config, cross_packages_path):
    """Cross-compile site-packages for the serve platform (cross_platform only)."""
    if not config.cross_platform:
        return []
    src = "pyproject.toml" if config.has_pyproject else "requirements.txt"
    extra = " ".join(config.extra_dependencies)
    index = config.python_extra_index_url
    return [
        run(
            "uv pip compile {} --universal --extra-index-url {} --index-url=https://pypi.org/simple --emit-index-url --no-deps -o cross-requirements.txt".format(src, index),
            outputs = ["cross-requirements.txt"],
        ),
        run(
            "uvx pip install -r cross-requirements.txt {} --target {} --platform {} --only-binary=:all: --python-version={} --compile".format(extra, cross_packages_path, config.cross_platform, config.python_version),
        ),
        run("rm cross-requirements.txt"),
    ]

def python_commands(config, venv):
    """Start/after_deploy commands. Exported for reuse and testing."""
    asgi = config.asgi_application
    wsgi = config.wsgi_application
    main_file = config.main_file

    start = None
    if config.server == "daphne":
        start = f"daphne {asgi} --bind 0.0.0.0 --port {PORT}"
    elif config.server == "uvicorn":
        if asgi:
            start = f"uvicorn {asgi} --host 0.0.0.0 --port {PORT}"
        elif wsgi:
            start = f"uvicorn {wsgi} --interface=wsgi --host 0.0.0.0 --port {PORT}"
    elif config.server == "hypercorn":
        start = f"hypercorn {asgi} --bind 0.0.0.0:{PORT}"
    elif config.framework == "streamlit":
        start = f"streamlit run {main_file} --server.port {PORT} --server.address 0.0.0.0 --server.headless true"
    elif config.framework == "mcp":
        if config.mcp_self_running:
            start = "python " + main_file
        else:
            start = "python {}/bin/mcp run {} --transport=streamable-http".format(venv.serve_path, main_file)
    elif config.framework == "django":
        start = f"python manage.py runserver 0.0.0.0:{PORT}"
    if not start and main_file:
        start = "python " + main_file

    commands = {}
    if start:
        commands["start"] = start
    if config.migration_strategy == "django":
        commands["after_deploy"] = "python manage.py migrate"
    elif config.migration_strategy == "alembic":
        commands["after_deploy"] = "alembic upgrade head"
    return commands

def python_env(config, app, site_packages):
    app_path = app.serve_path
    if config.main_file and config.main_file.startswith("src/"):
        pythonpath = "{}:{}/src:{}".format(app_path, app_path, site_packages)
    else:
        pythonpath = "{}:{}".format(app_path, site_packages)
    env_vars = {"PYTHONPATH": pythonpath, "HOME": app_path}
    if config.framework == "streamlit":
        env_vars["STREAMLIT_SERVER_HEADLESS"] = "true"
    elif config.framework == "mcp":
        env_vars["FASTMCP_HOST"] = "0.0.0.0"
        env_vars["FASTMCP_PORT"] = PORT
    return env_vars

def python_prepare(config, site_packages, app_serve_path):
    if not config.precompile_python:
        return []
    return [
        run('echo "Precompiling Python code..."'),
        run("python -m compileall -o 2 {} || true".format(site_packages)),
        run('echo "Precompiling package code..."'),
        run("python -m compileall -o 2 {} || true".format(app_serve_path)),
    ]

def python_services(config):
    if config.database == "mysql":
        return [service(name = "database", provider = "mysql")]
    if config.database == "postgresql":
        return [service(name = "database", provider = "postgres")]
    return []

def python_build(config, source = None):
    """Build-only fragment (replaces PythonProvider(only_build=True)).

    Installs dependencies into a local venv without wiring a serve. Returns a
    struct so composing providers (mkdocs, ...) can pick what they need.
    """
    tc = python_toolchain(config)
    src = source or mount("temp")
    local_venv = mount("local_venv")
    venv = local_venv  # build-only: everything targets the local venv

    steps = [use(tc.python, tc.uv)]
    steps += _subdir_steps(config, src)
    steps += _install_steps(config, venv, local_venv)
    steps += [
        path((local_venv.path if config.cross_platform else venv.path) + "/bin"),
        copy(".", ".", ignore = [".venv", ".git", "__pycache__"]) if not config.install_requires_all_files else None,
    ]
    return struct(
        steps = compact(steps),
        python = tc.python,
        uv = tc.uv,
        source = src,
        venv = venv,
    )

def python_serve(
        config,
        name = None,
        build_pre = [],
        build_post = [],
        extra_deps = [],
        extra_env = {},
        extra_commands = {},
        prepare = None,
        services = None):
    """Serve a Python app — the whole PythonProvider, as one call."""
    tc = python_toolchain(config)
    python_version = config.python_version
    cross = config.cross_platform
    in_subdir = config.app_subdir != None

    temp = mount("temp") if in_subdir else None
    app = mount("app")
    venv = mount("venv")
    local_venv = mount("local_venv")

    site_packages = "{}/lib/python{}/site-packages".format(venv.serve_path, python_version)
    cross_packages = "{}/lib/python{}/site-packages".format(venv.path, python_version)

    build = [use(tc.python, tc.uv)]
    build += _subdir_steps(config, temp if in_subdir else app)
    build += _install_steps(config, venv, local_venv)
    build += _cross_wheel_steps(config, cross_packages)
    build += [
        path((local_venv.path if cross else venv.path) + "/bin"),
        copy(".", ".", ignore = [".venv", ".git", "__pycache__"]) if not in_subdir and not config.install_requires_all_files else None,
    ]
    if config.framework == "mcp" and cross:
        build += [
            run("mkdir -p {}/bin".format(venv.path)),
            run("cp {}/bin/mcp {}/bin/mcp".format(local_venv.path, venv.path)),
        ]
    if config.framework == "django":
        build.append(run("python manage.py collectstatic --noinput", group = "build"))
    if in_subdir:
        build.append(run("cp -R . {}".format(app.path)))

    runtime_deps = [tc.python]
    if config.uses_pandoc:
        runtime_deps.append(dep("pandoc", config.pandoc_version))
    if config.uses_ffmpeg:
        runtime_deps.append(dep("ffmpeg", config.ffmpeg_version))

    return app_serve(
        config,
        provider = "python",
        name = name,
        cwd = app.serve_path,
        build = build,
        build_pre = build_pre,
        build_post = build_post,
        deps = runtime_deps,
        extra_deps = extra_deps,
        prepare = prepare if prepare != None else python_prepare(config, site_packages, app.serve_path),
        env = python_env(config, app, site_packages),
        extra_env = extra_env,
        commands = python_commands(config, venv),
        extra_commands = extra_commands,
        services = services if services != None else python_services(config),
        mounts = [app, venv],
    )
```

New `PythonConfig` fields this requires (all computed in `load_config()`,
replacing generation-time `_exists()` calls inside `build_steps()`):

```python
name: str                                   # serve name (dir name) — on base Config, set by CLI
has_pyproject: bool = False
has_requirements: bool = False
has_uv_lock: bool = False
install_inputs: Optional[List[str]] = None  # from discover_python_install_context
mcp_self_running: bool = False              # main file calls mcp.run()/__main__ itself
```

### `//shipit/tools:staticfile.shipit`

The base of the whole static-site family. Note `static_app` can be passed in —
that's the composition hook.

```python
load("//shipit:serve.shipit", "app_serve")

def staticfile_serve(
        config,
        name = None,
        provider = "staticfile",
        static_app = None,     # pass a pre-created mount to reference it in `build`
        build = None,          # override to produce the site instead of copying it
        serve_deps = [],
        **kwargs):
    """Serve a directory with static-web-server."""
    sws = dep("static-web-server", config.sws_version)
    static_app = static_app or mount("static_app")
    if build == None:
        build = [
            workdir(static_app.path),
            copy(config.static_dir or ".", ".", ignore = [".git"]),
        ]
    return app_serve(
        config,
        provider = provider,
        name = name,
        build = build,
        deps = serve_deps + [sws],
        commands = {
            "start": "static-web-server --root={} --log-level=info --port={}".format(static_app.serve_path, PORT),
        },
        mounts = [static_app],
        **kwargs
    )
```

### `//shipit/tools:mkdocs.shipit` — composition instead of string-gluing

Today this is an 87-line Python class delegating to two providers through
emitted strings. It becomes:

```python
load("//shipit/tools:python.shipit", "python_build")
load("//shipit/tools:staticfile.shipit", "staticfile_serve")

def mkdocs_serve(config, name = None, **kwargs):
    py = python_build(config)
    static_app = mount("static_app")
    return staticfile_serve(
        config,
        name = name,
        provider = "mkdocs",
        static_app = static_app,
        build = py.steps + [
            run("uv run mkdocs build --site-dir={}".format(static_app.path), outputs = ["."], group = "build"),
        ],
        serve_deps = [py.python],
        **kwargs
    )
```

`hugo.shipit` and `jekyll.shipit` follow the same ~15-line shape;
`node_static.shipit` is `node_build` + `staticfile_serve`.

### `//shipit/tools:php.shipit` — hook points become keywords

`PhpProvider.build_steps_with_options(extra_ignore, after_install, after_build)`
already invented the hook pattern in Python; here it becomes plain kwargs.

```python
load("//shipit:prelude.shipit", "merged")
load("//shipit:serve.shipit", "app_serve")

def php_build(config, app, assets, after_install = [], after_build = [], extra_ignore = []):
    steps = [workdir(app.path)]
    if config.has_php_ini:
        steps.append(copy("php.ini", "{}/php.ini".format(assets.path)))
    else:
        steps.append(copy("php/php.ini", "{}/php.ini".format(assets.path), base = "assets"))
    if config.use_composer:
        steps.append(env(COMPOSER_HOME = "/tmp", COMPOSER_FUND = "0", COMPOSER_ALLOW_SUPERUSER = "1"))
        steps.append(run(
            "composer install --optimize-autoloader --ignore-platform-reqs --no-scripts --no-interaction",
            inputs = ["composer.json", "composer.lock"], outputs = ["."], group = "install",
        ))
    steps += after_install

    ignore = [".git"] + extra_ignore
    if config.use_composer:
        ignore.append("vendor")
    if config.framework == "symfony":
        ignore.append("var")
    steps.append(copy(".", ignore = ignore))

    if config.use_composer and config.composer_build_script:
        steps.append(run("composer run-script {}".format(config.composer_build_script), outputs = ["."], group = "build"))
    return steps + after_build

def php_commands(config, app, assets):
    engine = "phpix" if config.phpix else "php"
    docroot = app.serve_path
    if config.public_dir:  # "public" / "app" / "web" — detected Python-side
        docroot = "{}/{}".format(app.serve_path, config.public_dir)
    return {"start": "{} -S localhost:{} -t {}".format(engine, PORT, docroot)}

def php_serve(
        config,
        name = None,
        provider = "php",
        app = None,
        assets = None,
        build_pre = [],          # after use(), before php's own steps
        after_install = [],
        after_build = [],
        extra_ignore = [],
        extra_mounts = [],
        **kwargs):
    php = dep("php", config.php_version, architecture = config.php_architecture)
    app = app or mount("app")
    assets = assets or mount("assets")

    env_vars = {"PHP_INI_SCAN_DIR": "{}".format(assets.serve_path)}
    serve_deps = [php]
    if config.phpix:
        serve_deps = [dep("phpix", config.php_version, architecture = config.php_architecture)]
        if config.phpix_worker_threads:
            env_vars["PHPIX_PHP_THREADS"] = str(config.phpix_worker_threads)
    if config.use_composer:
        serve_deps.append(dep("bash"))

    return app_serve(
        config,
        provider = provider,
        name = name,
        cwd = app.serve_path,
        build = [use(php)] + list(build_pre) + php_build(config, app, assets, after_install, after_build, extra_ignore),
        deps = serve_deps,
        env = env_vars,
        commands = php_commands(config, app, assets),
        mounts = [app, assets] + extra_mounts,
        **kwargs
    )
```

### `//shipit/tools:wordpress.shipit` — the higher-order serve in action

Today: a `PhpProvider` subclass overriding five emitter methods. After:
one function that says exactly what WordPress adds on top of PHP.

```python
load("//shipit/tools:php.shipit", "php_serve")

def wordpress_serve(config, name = None):
    app = mount("app")
    assets = mount("assets")
    wpcontent_base = mount("wpcontent_base")
    wp_content = volume("wp-content", "{}/wp-content/".format(app.serve_path))

    cli_ver = config.wp_cli_version
    if cli_ver:
        wp_cli_url = "https://github.com/wp-cli/wp-cli/releases/download/v{}/wp-cli-{}.phar".format(cli_ver, cli_ver)
    else:
        wp_cli_url = "https://raw.githubusercontent.com/wp-cli/builds/gh-pages/phar/wp-cli.phar"

    return php_serve(
        config,
        name = name,
        provider = "wordpress",
        app = app,
        assets = assets,
        build_pre = [
            copy(wp_cli_url, "{}/wp-cli.phar".format(assets.path)),
            copy("wordpress/install.sh", "{}/setup-wp.sh".format(assets.path), base = "assets"),
            run("php -d memory_limit=512M {}/wp-cli.phar core download --allow-root --path={} --version={}".format(assets.path, app.path, config.wp_version)),
            copy("wordpress/start.php", "{}/start-wp.php".format(assets.path), base = "assets"),
            copy("wordpress/wp-config.php", "{}/wp-config.php".format(app.path), base = "assets"),
            copy("wordpress/.htaccess", "{}/.htaccess".format(app.path), base = "assets"),
        ],
        extra_ignore = ["wp-content"],
        after_build = [
            run("cp -R {}/wp-content/* {}".format(app.path, wpcontent_base.path)),
        ],
        extra_mounts = [wpcontent_base],
        volumes = [wp_content],
        services = [service(name = "database", provider = "mysql")],
        extra_env = {
            "PAGER": "cat",
            "WPCONTENT_BASE_PATH": "{}".format(wpcontent_base.serve_path),
        },
        extra_commands = {
            "wp": "php {}/wp-cli.phar --allow-root --path={}".format(assets.serve_path, app.serve_path),
            "after_deploy": "bash {}/setup-wp.sh".format(assets.serve_path),
            "start": "phpix --startup-script={}/start-wp.php -S localhost:{} -t {}".format(assets.serve_path, PORT, app.serve_path),
        },
    )
```

The convention this establishes: **mounts are created by the outermost caller
that needs to reference them, and threaded down** (`app`/`assets` here,
`static_app` in mkdocs). Providers create their mounts only when not given one.

---

## User customization

Because the generated file is real Starlark calling documented functions, the
extension story falls out for free — no more "regenerate and lose your edits".

Add a build step and an env var:

```python
load("//shipit/tools:python.shipit", "python_serve")

python_build_and_serve(
    config,
    extra_env = {"DJANGO_SETTINGS_MODULE": "mysite.settings.prod"},
    build_post = [
        run("python manage.py compress", group = "build"),
    ],
)
```

Swap the start command but keep everything else:

```python
python_build_and_serve(config, extra_commands = {
    "start": "gunicorn mysite.wsgi -b 0.0.0.0:$PORT",
})
```

Project-local provider library (a team encodes their own conventions once):

```python
# deploy/acme.shipit
load("//shipit/tools:python.shipit", "python_build_and_serve")

def acme_python_serve(config, **kwargs):
    return python_build_and_serve(
        config,
        extra_env = {"SENTRY_ENVIRONMENT": "production"},
        build_post = [run("python -m acme_healthcheck", group = "build")],
        **kwargs
    )
```

```python
# Shipit
load("//deploy:acme.shipit", "acme_python_serve")

acme_python_serve(config)
```

Full control remains available: nothing stops a user from ignoring the stdlib
and writing raw `serve(...)` exactly as today.

---

## Python side after the refactor

A provider shrinks to detection + config + a pointer at its entrypoint:

```python
class PythonProvider:
    shipit_module = "//shipit/tools:python.shipit"
    shipit_function = "python_serve"

    @classmethod
    def name(cls) -> str: ...
    @classmethod
    def detect(cls, path, config) -> Optional[DetectResult]: ...   # unchanged
    @classmethod
    def load_config(cls, path, base_config) -> PythonConfig: ...   # unchanged + new fields
```

Deleted once all providers are ported: `build_steps()`, `declarations()`,
`dependencies()`, `prepare_steps()`, `commands()`, `mounts()`, `volumes()`,
`env()`, `services()` on every provider; `DependencySpec`/`MountSpec`/
`VolumeSpec`/`ServiceSpec`/`ProviderPlan`; ~220 of `generator.py`'s 243 lines.
That's roughly 3,000 lines of string-emission Python replaced by ~1,000 lines
of readable Starlark.

`generate_shipit()` becomes:

```python
def generate_shipit(path, provider_cls, subdir=None) -> str:
    lines = [f'load("{provider_cls.shipit_module}", "{provider_cls.shipit_function}")', ""]
    if subdir:
        lines += [f"app_subdir = {json.dumps(subdir)}", ""]
    lines += [f"{provider_cls.shipit_function}(config)", ""]
    return "\n".join(lines)
```

`evaluate_shipit()` changes in three places: the extended dialect, the
extended globals (`struct` et al.), and `eval_module_graph()` instead of a bare
`eval_module()`. Its post-eval behavior (applying `config.commands.*`
overrides, `$PORT` substitution) is untouched.

---

## Verification

The port can be gated on **plan equivalence**, which is much stronger than
text goldens:

```python
@pytest.mark.parametrize("example_dir", _EXAMPLE_DIRS)
def test_plan_equivalence(example_dir):
    config = load_example_config(example_dir)
    old = evaluate_text(legacy_generate_shipit(...), config)   # today's inlined output
    new = evaluate_text('load("//shipit/tools:python.shipit", "python_build_and_serve")\npython_build_and_serve(config)', config)
    assert plan_json(old) == plan_json(new)   # Serve → normalized JSON (steps in order, deps, env, commands, mounts, volumes, services)
```

Every existing example becomes an equivalence case. A provider is "done" when
all its examples produce byte-identical plan JSON through both paths; only then
does its legacy emitter get deleted. The existing e2e tests (which actually
build and run the examples) stay as the final gate.

## Migration plan

- **Phase 0 — engine plumbing.** Extended dialect + globals, `starlark_loader.py`,
  `config_view()`, `name` on base `Config`. Old inline files still evaluate
  (superset). No behavior change.
- **Phase 1 — python vertical slice.** `prelude.shipit`, `serve.shipit`,
  `python.shipit`; new `PythonConfig` fields; `shipit_module` entrypoint on
  `PythonProvider`; generator emits the 2-liner for providers that declare an
  entrypoint. Plan-equivalence tests over all `python-*` examples.
- **Phase 2 — the static-site family.** `staticfile`, `hugo`, `jekyll`,
  `mkdocs` (first real composition), then `php` + `wordpress` + `laravel`
  (hooks + higher-order), then `go`.
- **Phase 3 — node.** `node.shipit` and `node_static.shipit` last — `node.py`
  is 1,266 lines and has the most config surface; by then the patterns and the
  equivalence harness are proven.
- **Phase 4 — deletion.** Remove legacy emitters, `ProviderPlan`, generator
  internals; regenerate example `Shipit` files to the 2-line form; docs for the
  stdlib API and user extension points.

## Risks and open questions

1. **The stdlib becomes public API.** User-edited `Shipit` files will call
   `python_serve(config, extra_env=...)` forever. Keyword names and semantics
   need semver discipline. Mitigations: keep the exported surface minimal
   (underscore-prefixed helpers are unloadable by Starlark's own rules), and
   funnel all providers through `app_serve` so the override vocabulary is
   uniform and documented once.
2. **Per-run evaluation of the module graph** (builtins bind a per-run `Ctx`).
   Measured cost is negligible for a handful of small files; if it ever grows,
   move to an explicit context object threaded through calls, which also
   unlocks cross-run caching of frozen modules.
3. **f-string limitation** (identifiers only) is a style footgun for stdlib
   authors. Enforced by the parser at load time, so failures are loud; the
   style rule is "bind attributes to locals first, or use `.format()`".
4. **Config completeness.** Any filesystem fact still probed at
   generation-time must become a config field, or the Starlark port silently
   diverges. The plan-equivalence tests are the safety net — divergence shows
   up as plan JSON diffs, example by example.
5. **Subdir persistence.** Keeping the `app_subdir = "..."` assignment in the
   generated file preserves `read_shipit_subdir()`'s regex. Cleaner long-term:
   persist it in config metadata rather than the Shipit file — out of scope
   here.
6. **Should `config` be ambient or explicit in user files?** This proposal
   keeps it ambient in the entry file (injected global, matching today) but
   explicit everywhere else (function parameter). Alternative — generating
   `python_serve(load_config())` — was rejected: config construction needs
   filesystem access, which Starlark deliberately doesn't have.
7. **Inline escape hatch.** Once emitters are deleted we can no longer generate
   the old fully-inlined file. If we want a `shipit eject`-style command, it
   would pretty-print the *evaluated plan* back into `serve(...)` syntax —
   doable, but a separate feature.
