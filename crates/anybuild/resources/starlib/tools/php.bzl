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
        composer = dep("composer") if config.composer_enable else None,
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
    if config.composer_enable:
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
    if config.composer_enable:
        steps.append(env(COMPOSER_HOME = "/tmp", COMPOSER_FUND = "0", COMPOSER_ALLOW_SUPERUSER = "1"))
        composer_inputs = ["composer.json"]
        if file_exists("composer.lock"):
            composer_inputs.append("composer.lock")
        steps.append(run(
            "composer install --optimize-autoloader --ignore-platform-reqs --no-scripts --no-interaction",
            inputs = composer_inputs,
            outputs = ["."],
            group = "install",
        ))
    steps += after_install

    ignore = [".git"] + list(extra_ignore)
    if config.composer_enable:
        ignore.append("vendor")
    if config.php_framework == "symfony":
        ignore.append("var")
    if config.php_framework == "typecho":
        ignore += ["usr", "config.inc.php"]
    steps.append(copy(".", ignore = ignore))

    # Composer scripts are skipped at install time, so run the build script after.
    if config.composer_enable and config.composer_build_script:
        steps.append(run("composer run-script {}".format(config.composer_build_script), outputs = ["."], group = "build"))
    steps += after_build
    mounts = [app, assets]
    env_vars = php_env(config, assets)
    usr_base = None
    if config.php_framework == "typecho":
        usr_base = mount("typecho_usr")
        mounts.append(usr_base)
        steps.append(copy("usr", usr_base.path))
        if file_exists("config.inc.php"):
            steps.append(copy("config.inc.php", "{}/config.inc.php".format(usr_base.path)))
        elif not file_exists("usr/config.inc.php"):
            steps.append(copy("php/typecho-config.inc.php", "{}/config.inc.php".format(usr_base.path), base = "assets"))
        steps.append(copy("php/start-typecho.php", "{}/start-typecho.php".format(assets.path), base = "assets"))
        if not config.phpix:
            steps.append(write(
                "{}/config.inc.php".format(app.path),
                "<?php\n$config = (getenv('TYPECHO_APP_PATH') ?: __DIR__) . '/usr/config.inc.php';\nif (is_file($config)) {\n    require_once $config;\n}\n",
            ))
        env_vars["TYPECHO_APP_PATH"] = app.serve_path
        env_vars["TYPECHO_STARTUP_SCRIPT"] = "{}/start-typecho.php".format(assets.serve_path)
        env_vars["TYPECHO_DB_ADAPTER"] = config.typecho_db_adapter or "Pdo_SQLite"

    serve_deps = php_runtime_deps(config, tc)
    if config.php_framework == "typecho" and not config.composer_enable:
        serve_deps.append(dep("bash"))
    return build(
        steps = steps,
        serve_deps = serve_deps,
        mounts = mounts,
        env = env_vars,
        app = app,
        assets = assets,
        php = tc.php,
        composer = tc.composer,
        typecho_usr = usr_base,
    )

def _quote(value):
    return "'" + value.replace("'", "'\"'\"'") + "'"

def php_commands(config, app, assets = None, typecho_usr = None):
    engine = "phpix" if config.phpix else "php"
    docroot = app.serve_path
    if config.php_public_dir:
        docroot = "{}/{}".format(app.serve_path, config.php_public_dir)
    commands = {"start": "{} -S 0.0.0.0:{} -t {}".format(engine, config.port, docroot)}
    if config.php_framework == "typecho":
        assets = assets or mount("assets")
        if config.phpix:
            commands["start"] = "phpix --startup-script={} -S 0.0.0.0:{} -t {}".format(_quote("{}/start-typecho.php".format(assets.serve_path)), config.port, docroot)
        typecho_usr = typecho_usr or mount("typecho_usr")
        initialize = "mkdir -p {} && cp -Rn --no-preserve=mode {} {}".format(
            _quote("{}/usr/uploads".format(app.serve_path)),
            _quote("{}/.".format(typecho_usr.serve_path)),
            _quote("{}/usr/".format(app.serve_path)),
        )
        commands["after_deploy"] = "bash -c " + _quote(initialize)
        install = "php " + _quote("{}/install.php".format(app.serve_path))
        if config.phpix:
            install = "php -r " + _quote("require getenv('TYPECHO_STARTUP_SCRIPT'); require getenv('TYPECHO_APP_PATH') . '/install.php';")
        commands["install"] = "bash -c " + _quote(initialize + " && " + install)
    return commands

def php_serve(config, build, name = None, provider = None, commands = None, **overrides):
    """Serve a PHP build with the php (or phpix) dev server."""
    app = build.app
    volumes = []
    if config.php_framework == "typecho":
        usr = volume("typecho-usr", "{}/usr".format(app.serve_path))
        volumes = [usr]
    return serve(
        config,
        build,
        provider = provider,
        name = name,
        cwd = app.serve_path,
        commands = commands if commands != None else php_commands(config, app, build.assets, build.typecho_usr),
        volumes = volumes,
        **overrides
    )
