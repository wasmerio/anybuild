"""Statically-exported Node sites: node build into a static-web-server serve.

Composition of the node build fragments with the staticfile serve — the
node toolchain never reaches the serve environment.
"""

load("//anybuild:prelude.bzl", "compact")
load("//anybuild:serve.bzl", "build")
load(
    "//anybuild/tools:node.bzl",
    "node_build_step",
    "node_copy_step",
    "node_install_steps",
    "node_stage_steps",
    "node_toolchain",
)
load(
    "//anybuild/tools:staticfile.bzl",
    "staticfile_serve",
    "sws_config_mount",
    "sws_config_step",
)

def nodestatic_config(schema = 1, **kwargs):
    return config(provider = "node-static", schema = schema, **kwargs)

def nodestatic_build(config, static_app = None):
    """Build the site with node into static_app; nothing of node is served."""
    tc = node_toolchain(config, serving = False)
    temp = mount("temp")
    static_app = static_app or mount("static_app")
    static_config = sws_config_mount(config)

    steps = [use(*tc.build_deps)] if tc.build_deps else []
    steps += node_stage_steps(config, temp)
    steps += node_install_steps(config)
    steps.append(node_copy_step(config))
    steps += node_build_step(
        config,
        outputs = [config.static_dir],
        static = True,
        node_server = config.node_server,
        node_framework = config.node_framework,
    )
    steps.append(run("cp -R {}/* {}/".format(config.static_dir, static_app.path)))
    if static_config != None:
        steps.append(sws_config_step(config, static_config))

    return build(
        steps = compact(steps),
        mounts = [static_app] + ([static_config] if static_config != None else []),
        static_app = static_app,
        static_config = static_config,
    )
