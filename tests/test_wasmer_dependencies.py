from shipit.runners.wasmer import WasmerRunner


def test_phpix_dependencies_use_exact_current_release() -> None:
    phpix = WasmerRunner.mapper["phpix"]
    dependencies = list(phpix["dependencies"].values())

    for arch_dependencies in phpix["architecture_dependencies"].values():
        dependencies.extend(arch_dependencies.values())

    phpix_dependencies = [
        dependency
        for dependency in dependencies
        if dependency.startswith("phpix/")
    ]

    assert phpix_dependencies
    assert all(
        dependency.endswith("@=0.2.2") for dependency in phpix_dependencies
    )
