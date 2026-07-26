"""Go apps: compile a static binary, serve it directly."""

load("//anybuild:serve.bzl", "build", "serve")

def go_config(schema = 1, **kwargs):
    return config(provider = "go", schema = schema, **kwargs)

def go_build(config, app = None, temp = None):
    """Compile the Go binary in the temp mount and copy it into app."""
    go = dep("go", config.go_version)
    temp = temp or mount("temp")
    app = app or mount("app")
    in_subdir = config.app_subdir != None
    build_dir = "{}/{}".format(temp.path, config.app_subdir) if in_subdir else temp.path

    steps = [workdir(temp.path)]
    if in_subdir:
        steps += [
            copy(".", ".", ignore = [".git"]),
            workdir(build_dir),
        ]
    steps.append(use(go))
    if not in_subdir:
        steps.append(copy(".", ".", ignore = [".git"]))

    steps += [
        env(GOCACHE = "/tmp/.cache/go-build", GOPATH = build_dir),
        run("go build -o {} {}".format(config.serve_binary, config.go_build_file), group = "build"),
    ]
    if in_subdir:
        steps.append(run("cp {} {}/{}".format(config.serve_binary, app.path, config.serve_binary)))
    else:
        steps.append(run("cp {}/{} {}/{}".format(temp.path, config.serve_binary, app.path, config.serve_binary)))

    return build(
        steps = steps,
        mounts = [app],
        app = app,
        temp = temp,
    )

def go_serve(config, build, name = None, provider = None, **overrides):
    """Serve the compiled binary from the app mount."""
    app = build.app
    return serve(
        config,
        build,
        provider = provider,
        name = name,
        cwd = app.serve_path,
        commands = {"start": "{}/{}".format(app.serve_path, config.serve_binary)},
        **overrides
    )
