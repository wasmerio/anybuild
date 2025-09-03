class Serve:
    def __init__(self, name, provider, build, deps, commands, assets=None, prepare=None, workers=None, mounts=None):
        self.name = name
        self.provider = provider
        self.build = build
        self.deps = deps
        self.commands = commands
        self.assets = assets
        self.workers = workers
        self.prepare = prepare
        self.mounts = mounts

    def __str__(self):
        return f"Serve(name={self.name}, provider={self.provider}, prepare={self.prepare}, deps={self.deps}, commands={self.commands}, workers={self.workers}, volumes={self.volumes})"