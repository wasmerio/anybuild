"""Hugo sites: build with hugo, serve with static-web-server."""

load("//anybuild:serve.bzl", "build")
load("//anybuild/tools:staticfile.bzl", "staticfile_serve", "sws_config_mount")

def hugo_build(config, static_app = None):
    """Build the site with hugo into the static_app mount."""
    hugo = dep("hugo", config.hugo_version)
    temp = mount("temp")
    static_app = static_app or mount("static_app")
    static_config = sws_config_mount(config)

    return build(
        steps = [
            use(hugo),
            workdir(temp.path),
            copy(".", ".", ignore = [".git"]),
            run("hugo --gc --minify", group = "build"),
            run("cp -R {}/* {}/".format(config.static_dir, static_app.path)),
        ],
        mounts = [static_app] + ([static_config] if static_config != None else []),
        static_app = static_app,
        static_config = static_config,
    )
