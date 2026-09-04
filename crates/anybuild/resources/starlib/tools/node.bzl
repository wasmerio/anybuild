"""Node apps: package-manager install, optional build, optimized node_modules.

Granular pieces (node_install_steps, node_copy_step, node_build_step,
node_optimize_steps) are exported so composing providers (node-static,
laravel) can reuse exactly the fragments they need.
"""

load("//anybuild:prelude.bzl", "compact")
load("//anybuild:serve.bzl", "build", "serve")

def node_config(schema = 1, **kwargs):
    return config(provider = "node", schema = schema, **kwargs)

_LOCKFILES = {
    "npm": ["package-lock.json"],
    "pnpm": ["pnpm-lock.yaml"],
    "yarn": ["yarn.lock"],
    "bun": ["bun.lock", "bun.lockb"],
}

_INSTALL_COMMANDS = {
    "npm": "npm install",
    "pnpm": "pnpm install",
    "yarn": "yarn install",
    "bun": "bun install",
}

_PRUNE_COMMANDS = {
    "npm": "npm prune --omit=dev --ignore-scripts",
    "pnpm": "pnpm prune --prod",
    "yarn": "yarn workspaces focus --all --production",
    "bun": "rm -rf node_modules && bun install --omit=dev --ignore-scripts",
}

_DLX_PREFIXES = {
    "npm": "npx -y ",
    "pnpm": "pnpm dlx ",
    "yarn": "yarn dlx ",
    "bun": "bunx ",
}

# Frameworks whose node_modules can be shrunk further after the build.
_FRAMEWORK_OPTIMIZE_DEPS_PATHS = {
    "astro": ["dist"],
}
_SERVER_OPTIMIZE_DEPS_PATHS = {
    "nitro": [".output/server"],
}
OPTIMIZE_DEPS_VERSION = "0.1.2"
NODE_MODULES_OPTIMIZER_ASSET = "node/optimize-node-modules.sh"

def _manager_version(config):
    manager = config.node_package_manager
    if manager == "npm":
        return config.npm_version
    if manager == "pnpm":
        return config.pnpm_version
    if manager == "yarn":
        return config.yarn_version
    if manager == "bun":
        return config.bun_version
    return None

def _lockfile(manager):
    for lockfile in _LOCKFILES[manager]:
        if file_exists(lockfile):
            return lockfile
    return None

def node_toolchain(config, serving = True):
    """Build-time packages: node, the package manager, and extras."""
    node = dep("node", config.node_version)
    has_package_json = file_exists("package.json")
    build_deps = []
    if has_package_json or config.node_build_command:
        build_deps.append(node)
    if config.node_package_manager and has_package_json:
        build_deps.append(dep(config.node_package_manager, _manager_version(config)))
        if serving and config.node_remove_native_binaries:
            build_deps.append(dep("bash"))
    for extra in config.node_extra_dependencies:
        build_deps.append(dep(extra))
    return struct(node = node, build_deps = build_deps)

def node_stage_steps(config, source):
    """Enter the build context (and the app subdirectory when present)."""
    if not config.app_subdir:
        return [workdir(source.path)]
    return [
        workdir(source.path),
        copy(".", ".", ignore = [".git", "node_modules"]),
        workdir("{}/{}".format(source.path, config.app_subdir)),
    ]

def node_install_steps(config):
    """Lockfile staging, package-manager env, and the install run."""
    if not file_exists("package.json"):
        return []
    manager = config.node_package_manager
    lockfile = _lockfile(manager)
    has_lockfile = lockfile != None
    install = _INSTALL_COMMANDS[manager]
    if manager == "bun" and has_lockfile:
        install += " --no-save"
    if config.app_subdir and manager == "pnpm":
        install += " --no-frozen-lockfile"
    all_files = config.node_install_requires_all_files

    steps = []
    if config.app_subdir:
        if has_lockfile:
            steps.append(copy("{}/{}".format(config.app_subdir, lockfile), lockfile))
    elif all_files:
        steps.append(copy(".", ".", ignore = ["node_modules", ".git"]))
    elif has_lockfile:
        steps.append(copy(lockfile))

    if manager == "pnpm":
        pnpm_env = {
            "pnpm_config_minimum_release_age": "0",
            "CI": "true",
        }
        if config.app_subdir:
            pnpm_env["pnpm_config_inject_workspace_packages"] = "true"
        pnpm_env["pnpm_config_dangerously_allow_all_builds"] = "true"
        steps.append(env(**pnpm_env))
    elif manager == "npm":
        steps.append(env(CI = "true", NPM_CONFIG_FUND = "false"))

    if config.app_subdir or all_files:
        steps.append(run(install, group = "install"))
    else:
        steps.append(run(install, inputs = config.node_install_inputs, group = "install"))
    return steps

def node_copy_step(config):
    """Copy the sources after install (skipped when install saw all files)."""
    if config.app_subdir or config.node_install_requires_all_files:
        return None
    ignore = ["node_modules", ".git"]
    if config.node_package_manager:
        lockfile = _lockfile(config.node_package_manager)
        if lockfile:
            ignore.append(lockfile)
    return copy(".", ignore = ignore)

def node_build_step(
        config,
        outputs = ["."],
        serving = True,
        static = False,
        node_server = None,
        node_framework = None):
    if not config.node_build_command:
        return []
    steps = []
    if node_server == "nitro":
        static_nitro = static and node_framework != "tanstack-start"
        steps.append(env(NITRO_PRESET = "static" if static_nitro else "node-server"))
    if serving:
        steps.append(run(config.node_build_command, outputs = outputs, group = "build"))
    else:
        steps.append(run(config.node_build_command, group = "build"))
    return steps

def node_optimize_steps(config, assets = None, include_prune = True, serving = True):
    """Prune dev deps and shrink node_modules for the serve environment."""
    if not file_exists("package.json") or not config.node_package_manager:
        return []
    steps = []
    if include_prune:
        steps.append(run(_PRUNE_COMMANDS[config.node_package_manager], group = "prune"))
    optimize_paths = []
    if config.node_build_command:
        optimize_paths = _FRAMEWORK_OPTIMIZE_DEPS_PATHS.get(
            config.node_framework,
            _SERVER_OPTIMIZE_DEPS_PATHS.get(config.node_server, []),
        )
    if optimize_paths and config.optimize_node_dependencies:
        steps.append(run(_DLX_PREFIXES[config.node_package_manager] + "optimize-deps@{} {} --replace".format(OPTIMIZE_DEPS_VERSION, ", ".join(optimize_paths))))
    if serving and config.node_remove_native_binaries:
        node_modules_path = ".next-bundle/node_modules" if config.node_framework == "next" else "node_modules"
        steps += [
            run("mkdir -p {}".format(assets.path), group = "optimize"),
            copy(NODE_MODULES_OPTIMIZER_ASSET, "{}/optimize-node-modules.sh".format(assets.path), base = "assets"),
            run("bash {}/optimize-node-modules.sh {}".format(assets.path, node_modules_path), group = "optimize"),
        ]
    return steps

def _uses_pnpm_deploy(config):
    return config.app_subdir and config.node_package_manager == "pnpm" and config.node_package_name

def _export_steps(config, build_mount, app):
    """Move the built app from the build mount into the served app mount."""
    if _uses_pnpm_deploy(config):
        return [
            workdir("{}/{}".format(build_mount.path, config.app_subdir)),
            run("pnpm deploy --filter {} --prod --config.node-linker=hoisted {}".format(config.node_package_name, app.path)),
            workdir(app.path),
        ]
    copy_source = ".next-bundle" if config.node_framework == "next" else "."
    copy_flags = "-RL" if config.app_subdir else "-R"
    return [run("cp {} {} {}".format(copy_flags, copy_source, app.path))]

def node_build(config, build_mount = None, app = None):
    """Install, build, optimize, and export into the app mount."""
    tc = node_toolchain(config)
    build_mount = build_mount or mount("build")
    app = app or mount("app")
    assets = mount("assets") if config.node_remove_native_binaries else None

    uses_pnpm_deploy = _uses_pnpm_deploy(config)
    export = _export_steps(config, build_mount, app)
    optimize = node_optimize_steps(
        config, assets = assets, include_prune = not uses_pnpm_deploy
    )
    tail = export + optimize if uses_pnpm_deploy else optimize + export

    steps = [use(*tc.build_deps)] if tc.build_deps else []
    steps += node_stage_steps(config, build_mount)
    steps += node_install_steps(config)
    steps.append(node_copy_step(config))
    steps += node_build_step(
        config,
        node_server = config.node_server,
        node_framework = config.node_framework,
    )
    steps += tail

    return build(
        steps = compact(steps),
        serve_deps = [tc.node],
        mounts = [app],
        node = tc.node,
        build_mount = build_mount,
        app = app,
        assets = assets,
    )

def node_serve(
        config,
        build,
        name = None,
        provider = None,
        prepare = None,
        services = None,
        **overrides):
    """Serve the exported app with the start command detected at load time."""
    app = build.app
    commands = {}
    if config.commands.start:
        commands["start"] = config.commands.start
    if prepare == None:
        if config.edgejs_precompile:
            prepare = [run("edgejs --precompile {}".format(app.serve_path))]
        else:
            prepare = []
    return serve(
        config,
        build,
        provider = provider,
        name = name,
        cwd = app.serve_path,
        prepare = prepare,
        commands = commands,
        services = services,
        **overrides
    )
