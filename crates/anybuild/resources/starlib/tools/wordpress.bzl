"""WordPress: full sites, and plugin/theme extension projects.

The higher-order pattern in action: a full site is php_build() with WordPress
steps hooked in before (wp-cli, core download) and after (wp-content seed);
an extension project builds into the wp-content mount instead.
"""

load("//anybuild:prelude.bzl", "merged")
load("//anybuild:serve.bzl", "build", "serve")
load(
    "//anybuild/tools:php.bzl",
    "php_build",
    "php_commands",
    "php_env",
    "php_ini_steps",
    "php_runtime_deps",
    "php_toolchain",
    "php_use_deps",
)

def wordpress_config(schema = 1, **kwargs):
    return config(provider = "wordpress", schema = schema, **kwargs)

def _wp_cli_url(config):
    version = config.wp_cli_version
    if version:
        return "https://github.com/wp-cli/wp-cli/releases/download/v{}/wp-cli-{}.phar".format(version, version)
    return "https://raw.githubusercontent.com/wp-cli/builds/gh-pages/phar/wp-cli.phar"

def _wp_base_steps(config, app, assets):
    """wp-cli, install script, core download, and default config files."""
    steps = [
        copy(_wp_cli_url(config), "{}/wp-cli.phar".format(assets.path)),
        copy("wordpress/install.sh", "{}/setup-wp.sh".format(assets.path), base = "assets"),
    ]
    if config.wp_version:
        flags = "--version={}".format(config.wp_version)
        if config.wp_locale:
            flags += " --locale={}".format(config.wp_locale)
        steps.append(run("php -d memory_limit=512M {}/wp-cli.phar core download --allow-root --path={} {}".format(assets.path, app.path, flags)))
    if config.phpix:
        # Startup script wires the .htaccess handling phpix supports.
        steps.append(copy("wordpress/start.php", "{}/start-wp.php".format(assets.path), base = "assets"))
    is_extension = config.wp_extension_kind != None
    if is_extension or not file_exists("wp-config.php"):
        steps.append(copy("wordpress/wp-config.php", "{}/wp-config.php".format(app.path), base = "assets"))
    if is_extension or not file_exists(".htaccess"):
        steps.append(copy("wordpress/.htaccess", "{}/.htaccess".format(app.path), base = "assets"))
    return steps

def _wp_content_seed_steps(config, app, wpcontent_base):
    steps = []
    if config.wp_version:
        steps.append(run("cp -R {}/wp-content/* {}".format(app.path, wpcontent_base.path)))
    if file_exists("wp-content"):
        steps.append(copy("wp-content", wpcontent_base.path))
    return steps

def _wp_extension_steps(config, app, assets, wpcontent_base):
    """Build a plugin/theme project into the wp-content mount."""
    target = "{}/{}s/{}".format(wpcontent_base.path, config.wp_extension_kind, config.wp_extension_slug)
    ignore = [".git", ".source"]
    if config.use_composer:
        ignore.append("vendor")

    steps = _wp_base_steps(config, app, assets)
    steps += [workdir(app.path)]
    steps += php_ini_steps(config, assets)
    steps += _wp_content_seed_steps(config, app, wpcontent_base)
    steps.append(copy(".", target, ignore = ignore))

    if config.use_composer:
        steps += [
            workdir(target),
            env(COMPOSER_HOME = "/tmp", COMPOSER_FUND = "0", COMPOSER_ALLOW_SUPERUSER = "1"),
            run("composer install --optimize-autoloader --ignore-platform-reqs --no-scripts --no-interaction", group = "install"),
        ]
        if config.composer_build_script:
            steps.append(run("composer run-script {}".format(config.composer_build_script), outputs = ["."], group = "build"))
    return steps

def wordpress_build(config, app = None, assets = None, wpcontent_base = None):
    app = app or mount("app")
    assets = assets or mount("assets")
    wpcontent_base = wpcontent_base or mount("wpcontent_base")

    # WordPress always needs bash at serve time (setup-wp.sh); php_runtime_deps
    # already includes it when composer is used.
    extra_serve_deps = [dep("bash")] if not config.use_composer else []

    if config.wp_extension_kind != None:
        tc = php_toolchain(config)
        steps = [use(*php_use_deps(tc))] + _wp_extension_steps(config, app, assets, wpcontent_base)
        serve_deps = php_runtime_deps(config, tc)
        env_vars = php_env(config, assets)
    else:
        php = php_build(
            config,
            app = app,
            assets = assets,
            build_pre = _wp_base_steps(config, app, assets),
            after_build = _wp_content_seed_steps(config, app, wpcontent_base),
            extra_ignore = ["wp-content"],
        )
        steps = php.steps
        serve_deps = php.serve_deps
        env_vars = php.env

    return build(
        steps = steps,
        serve_deps = serve_deps + extra_serve_deps,
        mounts = [app, assets, wpcontent_base],
        env = env_vars,
        app = app,
        assets = assets,
        wpcontent_base = wpcontent_base,
    )

def wordpress_commands(config, app, assets):
    commands = php_commands(config, app)
    if config.phpix and "start" in commands:
        commands["start"] = "phpix --startup-script={}/start-wp.php -S localhost:{} -t {}".format(assets.serve_path, config.port, app.serve_path)
    return merged({
        "wp": "php {}/wp-cli.phar --allow-root --path={}".format(assets.serve_path, app.serve_path),
        "after_deploy": "bash {}/setup-wp.sh".format(assets.serve_path),
    }, commands)

def wordpress_env(config, wpcontent_base):
    env_vars = {
        "PAGER": "cat",
        "WPCONTENT_BASE_PATH": wpcontent_base.serve_path,
    }
    if config.wp_locale:
        env_vars["WP_LOCALE"] = config.wp_locale
    if config.wp_extension_kind == "plugin":
        env_vars["WP_PLUGINS_ACTIVATE"] = config.wp_extension_activate_target
    elif config.wp_extension_kind == "theme":
        env_vars["WP_DEFAULT_THEME"] = config.wp_extension_activate_target
    return env_vars

def wordpress_serve(config, build, name = None, provider = None, **overrides):
    app = build.app
    assets = build.assets
    wpcontent_base = build.wpcontent_base
    wp_content = volume("wp-content", "{}/wp-content/".format(app.serve_path))

    return serve(
        config,
        build,
        provider = provider,
        name = name,
        cwd = app.serve_path,
        env = wordpress_env(config, wpcontent_base),
        commands = wordpress_commands(config, app, assets),
        volumes = [wp_content],
        services = [service(name = "database", provider = "mysql")],
        **overrides
    )
