"""PHP apps: composer install, php/phpix dev-server serve.

php_build() exposes the hook points downstream providers compose on:
`build_pre` (after use(), before PHP's own steps), `after_install`,
`after_build`, and `extra_ignore` — wordpress and laravel are wrappers
around these.
"""

load("//anybuild:serve.bzl", "build", "serve")

def php_config(schema = 1, **kwargs):
    return config(provider = "php", schema = schema, **kwargs)

def php_toolchain(config):
    return struct(
        php = dep("php", config.php_version, architecture = config.php_architecture),
        composer = dep("composer") if config.use_composer else None,
    )

def php_use_deps(toolchain):
    deps = [toolchain.php]
    if toolchain.composer != None:
        deps.append(toolchain.composer)
    return deps

def php_runtime_deps(config, toolchain):
    """Serve-time packages: phpix or php, plus bash when composer is used."""
    deps = []
    if config.phpix:
        deps.append(dep("phpix", config.php_version, architecture = config.php_architecture))
    else:
        deps.append(toolchain.php)
    if config.use_composer:
        deps.append(dep("bash"))
    return deps

def php_env(config, assets):
    env_vars = {"PHP_INI_SCAN_DIR": assets.serve_path}
    if config.phpix and config.phpix_worker_threads:
        env_vars["PHPIX_PHP_THREADS"] = str(config.phpix_worker_threads)
    return env_vars

def php_ini_steps(config, assets):
    """Stage php.ini into the assets mount (project's own or the default)."""
    if file_exists("php.ini"):
        return [copy("php.ini", "{}/php.ini".format(assets.path))]
    return [copy("php/php.ini", "{}/php.ini".format(assets.path), base = "assets")]

def php_build(
        config,
        app = None,
        assets = None,
        build_pre = [],
        after_install = [],
        after_build = [],
        extra_ignore = [],
        extra_use_deps = []):
    """Stage sources and install composer dependencies."""
    tc = php_toolchain(config)
    app = app or mount("app")
    assets = assets or mount("assets")

    steps = [use(*(php_use_deps(tc) + list(extra_use_deps)))] + list(build_pre) + [workdir(app.path)]
    steps += php_ini_steps(config, assets)
    if config.use_composer:
        steps.append(env(COMPOSER_HOME = "/tmp", COMPOSER_FUND = "0", COMPOSER_ALLOW_SUPERUSER = "1"))
        steps.append(run(
            "composer install --optimize-autoloader --ignore-platform-reqs --no-scripts --no-interaction",
            inputs = ["composer.json", "composer.lock"],
            outputs = ["."],
            group = "install",
        ))
    steps += after_install

    ignore = [".git"] + list(extra_ignore)
    if config.use_composer:
        ignore.append("vendor")
    if config.framework == "symfony":
        ignore.append("var")
    steps.append(copy(".", ignore = ignore))

    # Composer scripts are skipped at install time, so run the build script after.
    if config.use_composer and config.composer_build_script:
        steps.append(run("composer run-script {}".format(config.composer_build_script), outputs = ["."], group = "build"))
    steps += after_build

    return build(
        steps = steps,
        serve_deps = php_runtime_deps(config, tc),
        mounts = [app, assets],
        env = php_env(config, assets),
        app = app,
        assets = assets,
        php = tc.php,
        composer = tc.composer,
    )

def php_commands(config, app):
    engine = "phpix" if config.phpix else "php"
    docroot = app.serve_path
    if config.public_dir:
        docroot = "{}/{}".format(app.serve_path, config.public_dir)
    return {"start": "{} -S localhost:{} -t {}".format(engine, config.port, docroot)}

def php_serve(config, build, name = None, provider = None, commands = None, **overrides):
    """Serve a PHP build with the php (or phpix) dev server."""
    app = build.app
    return serve(
        config,
        build,
        provider = provider,
        name = name,
        cwd = app.serve_path,
        commands = commands if commands != None else php_commands(config, app),
        **overrides
    )
