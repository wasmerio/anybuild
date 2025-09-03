class Package:
    def __init__(self, name, version=None):
        self.name = name
        self.version = version

    def __str__(self):
        return f"{self.name}@{self.version}"


class DependencyManager:
    def __init__(self, name):
        self.name = name

    def is_installed(self, package):
        return False

    def install(self, package):
        pass


class LocalDependencyManager(DependencyManager):
    def __init__(self):
        super().__init__("local")

    def is_available(self):
        return True

    def is_installed(self, package):
        return False

    def install(self, package):
        pass


class BrewDependencyManager(DependencyManager):
    def __init__(self):
        super().__init__("python")
    
    def is_available(self):
        return False

    def is_installed(self, package):
        return False

    def install(self, package):
        pass
