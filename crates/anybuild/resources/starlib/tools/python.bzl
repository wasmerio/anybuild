"""Python apps: uv-managed virtualenv, optional cross-platform wheels.

Everything here is a function of `config` (a frozen snapshot produced by
anybuild's Python-side detection) and read-only file_exists() probes of the
app source tree. Derived facts (framework, install inputs, versions) live
in config; simple existence checks use file_exists() directly.

  python_build(config)                the full serving build (mounts, uv
                                      install, cross wheels); serving=False
                                      gives the embeddable local-venv flavor
  python_serve(config, build)         wire the Python runtime around a build
"""

load("//anybuild:serve.bzl", "build", "serve")

def python_config(schema = 1, **kwargs):
    return config(provider = "python", schema = schema, **kwargs)

def python_toolchain(config):
    return struct(
        python = dep("python", config.python_version),
        uv = dep("uv", config.uv_version),
    )

def python_runtime_deps(toolchain):
    """Packages the serve environment needs."""
    return [toolchain.python]

def _stage_steps(config, source):
    """Enter the build context (and the app subdirectory when present)."""
    if not config.app_subdir:
        return [workdir(source.path)]
    return [
        workdir(source.path),
        copy(".", ".", ignore = [".git", ".venv", "__pycache__"]),
        workdir("{}/{}".format(source.path, config.app_subdir)),
    ]

def _install_steps(config, venv, local_venv):
    python_version = config.python_version
    cross = config.python_cross_platform
    extra = ", ".join(config.python_extra_dependencies)
    all_files = config.python_install_requires_all_files
    in_subdir = config.app_subdir != None
    inputs = None if (in_subdir or all_files) else config.python_install_inputs

    steps = []
    if file_exists("pyproject.toml"):
        lock = " --locked" if file_exists("uv.lock") else ""
        steps += [
            env(
                UV_PROJECT_ENVIRONMENT = local_venv.path if cross else venv.path,
                UV_PYTHON_PREFERENCE = "only-system",
                UV_PYTHON = f"python{python_version}",
            ),
            copy(".", ".") if all_files and not in_subdir else None,
            run("uv sync" + lock, inputs = inputs, group = "install"),
            copy("pyproject.toml") if not in_subdir and not all_files else None,
            run("uv add {}".format(extra), group = "install") if extra else None,
        ]
    elif file_exists("requirements.txt") or extra:
        steps += [
            env(UV_PROJECT_ENVIRONMENT = local_venv.path if cross else venv.path),
            run("uv init", inputs = [], outputs = ["uv.lock"], group = "install"),
            copy(".", ".", ignore = [".venv", ".git", "__pycache__"]) if all_files and not in_subdir else None,
        ]
        if file_exists("requirements.txt"):
            steps.append(run("uv add -r requirements.txt {}".format(extra), inputs = inputs, group = "install"))
        else:
            steps.append(run("uv add {}".format(extra), group = "install"))
    return steps

def _cross_wheel_steps(config, venv, local_venv):
    """Cross-install site-packages for the serve platform (cross_platform only)."""
    cross = config.python_cross_platform
    if not cross:
        return []
    python_version = config.python_version
    extra = ", ".join(config.python_extra_dependencies)
    index = config.python_extra_index_url
    default_index = config.python_index_url or "https://pypi.org/simple"
    all_files = config.python_install_requires_all_files
    in_subdir = config.app_subdir != None
    cross_packages = "{}/lib/python{}/site-packages".format(venv.path, python_version)

    if file_exists("pyproject.toml"):
        compile_step = run(
            "uv pip compile pyproject.toml --universal --extra-index-url {} --default-index {} --emit-index-url --no-deps -o cross-requirements.txt".format(index, default_index),
            outputs = ["cross-requirements.txt"],
        )
    else:
        inputs = None if (in_subdir or all_files) else config.python_install_inputs
        compile_step = run(
            "uv pip compile requirements.txt --python-version={} --universal --extra-index-url {} --default-index {} --emit-index-url --no-deps -o cross-requirements.txt".format(python_version, index, default_index),
            inputs = inputs,
            outputs = ["cross-requirements.txt"],
        )
    return [
        compile_step,
        run("uvx pip install -r cross-requirements.txt {} --target {} --platform {} --only-binary=:all: --python-version={} --compile".format(extra, cross_packages, cross, python_version)),
        run("rm cross-requirements.txt"),
    ]

def python_build(
        config,
        source = None,
        app = None,
        venv = None,
        serving = True):
    """Install dependencies with uv and stage the app sources.

    The default is the full serving build: app/venv mounts, cross-platform
    wheels, and the subdir export. serving=False gives the embeddable flavor
    (temp mount + local venv) that composing providers like mkdocs build on.
    """
    tc = python_toolchain(config)
    in_subdir = config.app_subdir != None
    local_venv = mount("local_venv")
    if serving:
        temp = mount("temp") if in_subdir else None
        app = app or mount("app")
        venv = venv or mount("venv")
        source = source or (temp if in_subdir else app)
    else:
        source = source or mount("temp")
        venv = venv or local_venv  # the embeddable flavor targets the local venv
    cross = config.python_cross_platform

    steps = [use(tc.python, tc.uv)]
    steps += _stage_steps(config, source)
    steps += _install_steps(config, venv, local_venv)
    # Cross wheels only make sense when an install branch ran (pyproject,
    # requirements, or extra deps) — a bare script app has nothing to compile.
    has_install = file_exists("pyproject.toml") or file_exists("requirements.txt") or len(config.python_extra_dependencies) > 0
    if serving and has_install:
        steps += _cross_wheel_steps(config, venv, local_venv)
    steps += [
        path((local_venv.path if cross else venv.path) + "/bin"),
        copy(".", ".", ignore = [".venv", ".git", "__pycache__"]) if not in_subdir and not config.python_install_requires_all_files else None,
    ]
    if config.python_framework == "mcp":
        steps += [
            run("mkdir -p {}/bin".format(venv.path)) if cross else None,
            run("cp {}/bin/mcp {}/bin/mcp".format(local_venv.path, venv.path)) if cross else None,
        ]
    if config.python_framework == "django":
        steps.append(run("python manage.py collectstatic --noinput", group = "build"))
    if serving and in_subdir:
        steps.append(run("cp -R . {}".format(app.path)))

    return build(
        steps = steps,
        serve_deps = python_runtime_deps(tc),
        python = tc.python,
        uv = tc.uv,
        app = app,
        source = source,
        venv = venv,
        local_venv = local_venv,
    )

def python_commands(config):
    """Commands resolved from the Python provider configuration."""
    commands = {}
    if config.commands.start:
        commands["start"] = config.commands.start
    if config.python_migration_strategy == "django":
        commands["after_deploy"] = "python manage.py migrate"
    elif config.python_migration_strategy == "alembic":
        commands["after_deploy"] = "alembic upgrade head"
    return commands

def python_env(config, app, venv, site_packages):
    app_path = app.serve_path
    if config.python_main_file and config.python_main_file.startswith("src/"):
        pythonpath = "{}:{}/src:{}".format(app_path, app_path, site_packages)
    else:
        pythonpath = "{}:{}".format(app_path, site_packages)
    env_vars = {"PYTHONPATH": pythonpath, "HOME": app_path}
    if config.python_framework == "streamlit":
        env_vars["STREAMLIT_SERVER_HEADLESS"] = "true"
    elif config.python_framework == "mcp":
        env_vars["FASTMCP_HOST"] = "0.0.0.0"
        env_vars["FASTMCP_PORT"] = str(config.port)
        if not config.python_mcp_self_running:
            env_vars["VIRTUAL_ENV"] = venv.serve_path
    return env_vars

def python_prepare(config, site_packages, app_serve_path):
    """Precompile site-packages and the app for faster cold starts."""
    if not config.python_precompile:
        return []
    return [
        run("echo \"Precompiling Python code...\""),
        run("python -m compileall -o 2 {} || true".format(site_packages)),
        run("echo \"Precompiling package code...\""),
        run("python -m compileall -o 2 {} || true".format(app_serve_path)),
    ]

def python_services(config):
    if config.python_database == "mysql":
        return [service(name = "database", provider = "mysql")]
    if config.python_database == "postgresql":
        return [service(name = "database", provider = "postgres")]
    return []

def python_serve(
        config,
        build,
        app = None,
        name = None,
        provider = None,
        prepare = None,
        services = None,
        **overrides):
    """Wire the Python runtime serve around a build struct."""
    app = app or build.app or build.source
    venv = build.venv
    site_packages = "{}/lib/python{}/site-packages".format(venv.serve_path, config.python_version)

    return serve(
        config,
        build,
        provider = provider,
        name = name,
        cwd = app.serve_path,
        prepare = prepare if prepare != None else python_prepare(config, site_packages, app.serve_path),
        env = python_env(config, app, venv, site_packages),
        commands = python_commands(config),
        services = services if services != None else python_services(config),
        mounts = [app, venv],
        **overrides
    )
