"""The higher-order serve.

Provider libraries follow a two-function convention:

  <provider>_build(config, ...)         -> build struct (see build())
  <provider>_serve(config, build, ...)  -> wire a serve around any build

The generated Anybuild file calls both, making the seam users compose on
explicit:

  build = python_build(config)
  python_serve(config, build)

Splitting build from serve is what makes providers composable: a provider
that inherits from another mixes builds and serves freely (mkdocs runs a
python build and a staticfile serve; wordpress wraps the php build with
extra steps and overrides parts of the php serve).

A build struct carries at least:

  steps       list of build steps (None entries allowed; filtered here)
  serve_deps  packages the runtime needs because of this build
  mounts      mounts created by the build that must attach to the serve
  env         env vars the serve must set because of this build

plus any provider-specific fields (mount handles etc.) for downstream serves.
"""

load("//anybuild:prelude.bzl", "compact", "merged")

def build(steps, serve_deps = [], mounts = [], env = {}, **extra):
    """Create a build struct with the convention fields defaulted."""
    return struct(
        steps = steps,
        serve_deps = serve_deps,
        mounts = mounts,
        env = env,
        **extra
    )

def _override_group_commands(config, steps):
    """Honor the user's --install/--build command overrides.

    The first run step of each group is replaced with the user's command;
    when a group has no step, the command is appended at the end (same
    semantics the CLI applied before plans were Starlark-determined).
    """
    if not config.commands.build and not config.commands.install:
        return steps
    new_steps = []
    done_build = False
    done_install = False
    for step in steps:
        command = getattr(step, "command", None)
        group = getattr(step, "group", None)
        if command != None and group == "build" and config.commands.build and not done_build:
            new_steps.append(run(config.commands.build, group = "build"))
            done_build = True
        elif command != None and group == "install" and config.commands.install and not done_install:
            new_steps.append(run(config.commands.install, group = "install"))
            done_install = True
        else:
            new_steps.append(step)
    if config.commands.install and not done_install:
        new_steps.append(run(config.commands.install, group = "install"))
    if config.commands.build and not done_build:
        new_steps.append(run(config.commands.build, group = "build"))
    return new_steps

def _override_serve_commands(config, commands):
    """User start/after_deploy overrides, plus $PORT substitution."""
    result = dict(commands)
    if config.commands.start:
        result["start"] = config.commands.start
    if config.commands.after_deploy:
        result["after_deploy"] = config.commands.after_deploy
    for key in ("start", "after_deploy"):
        if result.get(key):
            result[key] = result[key].replace("$PORT", str(config.port))
    return result

def serve(
        config,
        build,
        provider = None,
        name = None,
        commands = {},
        env = {},
        prepare = None,
        cwd = None,
        mounts = [],
        volumes = [],
        services = [],
        serve_deps = [],
        # Uniform user-facing override surface (same for every provider):
        build_pre = [],
        build_post = [],
        extra_deps = [],
        extra_env = {},
        extra_commands = {}):
    """Assemble a serve() from a build struct plus the serve-side wiring.

    User command overrides (config.commands.*) are applied here, so the
    evaluated plan is complete — the CLI does not edit it afterwards.
    """
    steps = compact(list(build_pre) + list(build.steps) + list(build_post))
    # This module's serve() shadows the raw builtin, so call its alias.
    return _serve(
        name = name or config.name,
        provider = provider or config.provider,
        runtime_port = config.port,
        cwd = cwd,
        build = _override_group_commands(config, steps),
        deps = list(build.serve_deps) + list(serve_deps) + list(extra_deps),
        prepare = compact(prepare) if prepare != None else None,
        env = merged(merged(build.env, env), extra_env),
        commands = _override_serve_commands(config, merged(commands, extra_commands)),
        mounts = list(build.mounts) + list(mounts),
        volumes = volumes,
        services = services,
    )
