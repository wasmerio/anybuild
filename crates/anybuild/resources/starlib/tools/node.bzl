"""Node apps: package-manager install, optional build, optimized node_modules.

Granular pieces (node_install_steps, node_copy_step, node_build_step,
node_optimize_steps) are exported so composing providers (node-static,
laravel) can reuse exactly the fragments they need.
"""

load("//anybuild:prelude.bzl", "compact")
load("//anybuild:serve.bzl", "build", "serve")

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
_OPTIMIZE_DEPS_PATHS = {"astro": ["dist"]}
OPTIMIZE_DEPS_VERSION = "0.1.1"
NODE_MODULES_OPTIMIZER_ASSET = "node/optimize-node-modules.sh"

def _manager_version(config):
    manager = config.package_manager
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
    if has_package_json or config.build_command:
        build_deps.append(node)
    if config.package_manager and has_package_json:
        build_deps.append(dep(config.package_manager, _manager_version(config)))
        if serving and config.remove_native_binaries:
            build_deps.append(dep("bash"))
    for extra in config.extra_dependencies:
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
    manager = config.package_manager
    lockfile = _lockfile(manager)
    has_lockfile = lockfile != None
    install = _INSTALL_COMMANDS[manager]
    if manager == "bun" and has_lockfile:
        install += " --no-save"
    if config.app_subdir and manager == "pnpm":
        install += " --no-frozen-lockfile"
    all_files = config.install_requires_all_files

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
        steps.append(run(install, inputs = config.install_inputs, group = "install"))
    return steps

def node_copy_step(config):
    """Copy the sources after install (skipped when install saw all files)."""
    if config.app_subdir or config.install_requires_all_files:
        return None
    ignore = ["node_modules", ".git"]
    if config.package_manager:
        lockfile = _lockfile(config.package_manager)
        if lockfile:
            ignore.append(lockfile)
    return copy(".", ignore = ignore)

def node_build_step(config, outputs = ["."], serving = True):
    if not config.build_command:
        return []
    steps = []
    if config.framework == "nitro":
        steps.append(env(NITRO_PRESET = "node-server"))
    if serving:
        steps.append(run(config.build_command, outputs = outputs, group = "build"))
    else:
        steps.append(run(config.build_command, group = "build"))
    return steps

def node_optimize_steps(config, assets = None, include_prune = True, serving = True):
    """Prune dev deps and shrink node_modules for the serve environment."""
    if not file_exists("package.json") or not config.package_manager:
        return []
    steps = []
    if include_prune:
        steps.append(run(_PRUNE_COMMANDS[config.package_manager], group = "prune"))
    optimize_paths = _OPTIMIZE_DEPS_PATHS.get(config.framework, [])
    if optimize_paths and config.optimize_node_dependencies:
        steps.append(run(_DLX_PREFIXES[config.package_manager] + "optimize-deps@{} {} --replace".format(OPTIMIZE_DEPS_VERSION, ", ".join(optimize_paths))))
    if serving and config.remove_native_binaries:
        steps += [
            run("mkdir -p {}".format(assets.path), group = "optimize"),
            copy(NODE_MODULES_OPTIMIZER_ASSET, "{}/optimize-node-modules.sh".format(assets.path), base = "assets"),
            run("bash {}/optimize-node-modules.sh node_modules".format(assets.path), group = "optimize"),
        ]
    return steps

def _export_steps(config, build_mount, app):
    """Move the built app from the build mount into the served app mount."""
    if config.app_subdir and config.package_manager == "pnpm" and config.package_name:
        return [
            workdir(build_mount.path),
            run("pnpm deploy --filter {} --prod --config.node-linker=hoisted {}".format(config.package_name, app.path)),
            workdir(app.path),
        ]
    copy_source = ".next-bundle/*" if config.framework == "next" else "."
    copy_flags = "-RL" if config.app_subdir else "-R"
    return [run("cp {} {} {}".format(copy_flags, copy_source, app.path))]

def node_build(config, build_mount = None, app = None):
    """Install, build, optimize, and export into the app mount."""
    tc = node_toolchain(config)
    build_mount = build_mount or mount("build")
    app = app or mount("app")
    assets = mount("assets") if config.remove_native_binaries else None

    uses_pnpm_deploy = (
        config.app_subdir and config.package_manager == "pnpm" and config.package_name
    )
    export = _export_steps(config, build_mount, app)
    optimize = node_optimize_steps(
        config, assets = assets, include_prune = not uses_pnpm_deploy
    )
    tail = export + optimize if uses_pnpm_deploy else optimize + export

    steps = [use(*tc.build_deps)] if tc.build_deps else []
    steps += node_stage_steps(config, build_mount)
    steps += node_install_steps(config)
    steps.append(node_copy_step(config))
    steps += node_build_step(config)
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

def node_serve(config, build, name = None, provider = "node", prepare = None, **overrides):
    """Serve the exported app with the start command detected at load time."""
    app = build.app
    commands = {}
    if config.commands.start:
        commands["start"] = config.commands.start
    if prepare == None:
        if config.precompile_edgejs:
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
        **overrides
    )
