from __future__ import annotations

from pathlib import Path
from typing import Dict, List, Optional

from shipit.providers.base import (
    DependencySpec,
    Provider,
    ProviderPlan,
    DetectResult,
    MountSpec,
    VolumeSpec,
    CustomCommands,
)
from shipit.providers.registry import providers as registry_providers


def _providers() -> list[type[Provider]]:
    # Load provider classes from modular registry
    return registry_providers()


def detect_provider(path: Path, custom_commands: CustomCommands) -> Provider:
    matches: list[tuple[type[Provider], DetectResult]] = []
    for provider_cls in _providers():
        res = provider_cls.detect(path, custom_commands)
        if res:
            matches.append((provider_cls, res))
    if not matches:
        raise Exception("Shipit could not detect a provider for this project")
    # Highest score wins; tie-breaker by order
    matches.sort(key=lambda x: x[1].score, reverse=True)
    return matches[0][0]


def _sanitize_alias(name: str) -> str:
    # Keep it predictable and valid in Starlark: letters, numbers, underscore
    # Remove dashes to keep prior style (e.g., staticwebserver)
    allowed = [c if c.isalnum() or c == "_" else "" for c in name]
    alias = "".join(allowed)
    return alias.replace("-", "")


def _emit_dependencies_declarations(
    deps: List[DependencySpec],
) -> tuple[str, List[str], List[str]]:
    lines: List[str] = []
    declared: set[str] = set()
    serve_vars: List[str] = []
    build_vars: List[str] = []

    for dep in deps:
        alias = dep.alias or _sanitize_alias(dep.name)

        # Track serve variables in order of appearance (deduped)
        if dep.use_in_serve and alias not in serve_vars:
            serve_vars.append(alias)
        if dep.use_in_build and alias not in build_vars:
            build_vars.append(alias)

        # Only declare each dependency once
        if alias in declared:
            continue
        declared.add(alias)

        version_var = None
        if dep.env_var:
            default = f' or "{dep.default_version}"' if dep.default_version else ""
            version_key = alias + "_version"
            lines.append(f'{version_key} = getenv("{dep.env_var}"){default}')
            version_var = version_key
        if version_var:
            lines.append(f'{alias} = dep("{dep.name}", {version_var})')
        else:
            lines.append(f'{alias} = dep("{dep.name}")')

    return "\n".join(lines), serve_vars, build_vars


def generate_shipit(path: Path, custom_commands: CustomCommands) -> str:
    provider_cls = detect_provider(path, custom_commands)
    provider = provider_cls(path, custom_commands)

    # Collect parts
    plan = ProviderPlan(
        serve_name=provider.serve_name(),
        provider=provider.provider_kind(),
        mounts=provider.mounts(),
        volumes=provider.volumes(),
        declarations=provider.declarations(),
        dependencies=provider.dependencies(),
        build_steps=provider.build_steps(),
        prepare=provider.prepare_steps(),
        services=provider.services(),
        commands=provider.commands(),
        env=provider.env(),
    )

    # Declare dependency variables (combined) and collect serve deps
    dep_block, serve_dep_vars, build_dep_vars = _emit_dependencies_declarations(
        plan.dependencies
    )

    # Compose serve(...) body
    # Auto-insert a use(...) step at the beginning if not explicitly provided
    build_steps: List[str] = list(plan.build_steps)
    if build_dep_vars and not any("use(" in s for s in build_steps):
        build_steps.insert(0, f"use({', '.join(build_dep_vars)})")

    build_steps_block = ",\n".join([f"    {s}" for s in build_steps])
    deps_array = ", ".join(serve_dep_vars)
    commands_lines = ",\n".join([f'    "{k}": {v}' for k, v in plan.commands.items()])
    env_lines = None
    if plan.env is not None:
        if len(plan.env) == 0:
            env_lines = "{}"
        else:
            env_lines = ",\n".join([f'    "{k}": {v}' for k, v in plan.env.items()])
    mounts_block = None
    volumes_block = None
    attach_serve_names: list[str] = []
    if plan.mounts:
        mounts = list(filter(lambda m: m.attach_to_serve, plan.mounts))
        attach_serve_names = [m.name for m in mounts]
        mounts_block = ",\n".join([f"    {m.name}" for m in mounts])
    if plan.volumes:
        volumes_block = ",\n".join(
            [f"    {v.var_name or v.name}" for v in plan.volumes]
        )

    out: List[str] = []
    if dep_block:
        out.append(dep_block)
        out.append("")
    for m in plan.mounts:
        out.append(f"{m.name} = mount(\"{m.name}\")")
    if plan.volumes:
        for v in plan.volumes:
            out.append(f"{v.var_name or v.name} = volume(\"{v.name}\", {v.serve_path})")
    if plan.services:
        for s in plan.services:
            out.append(f"{s.name} = service(\n  name=\"{s.name}\",\n  provider=\"{s.provider}\"\n)")

    if plan.declarations:
        out.append(plan.declarations)
        out.append("")
    out.append("serve(")
    out.append(f'  name="{plan.serve_name}",')
    out.append(f'  provider="{plan.provider}",')
    # If app is mounted for serve, set cwd to the app serve path
    if "app" in attach_serve_names:
        out.append('  cwd=app["serve"],')
    out.append("  build=[")
    out.append(build_steps_block)
    out.append("  ],")
    out.append(f"  deps=[{deps_array}],")
    if plan.prepare:
        prepare_steps_block = ",\n".join([f"    {s}" for s in plan.prepare])
        out.append("  prepare=[")
        out.append(prepare_steps_block)
        out.append("  ],")
    if env_lines is not None:
        if env_lines == "{}":
            out.append("  env = {},")
        else:
            out.append("  env = {")
            out.append(env_lines)
            out.append("  },")
    out.append("  commands = {")
    out.append(commands_lines)
    out.append("  },")
    if plan.services:
        out.append("  services=[")
        for s in plan.services:
            out.append(f"    {s.name},")
        out.append("  ],")
    if mounts_block:
        out.append("  mounts=[")
        out.append(mounts_block)
        out.append("  ],")
    if volumes_block:
        out.append("  volumes=[")
        out.append(volumes_block)
        out.append("  ],")
    out.append(")")
    out.append("")
    return "\n".join(out)
