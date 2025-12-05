import json
from pathlib import Path
from typing import List, Optional, Union

from shipit.providers.base import (
    DependencySpec,
    Provider,
    ProviderPlan,
    DetectResult,
    Config,
)
from shipit.providers.registry import providers as registry_providers


def _providers() -> list[type[Provider]]:
    # Load provider classes from modular registry
    return registry_providers()


def detect_provider(path: Path, base_config: Config) -> Provider:
    matches: list[tuple[type[Provider], DetectResult]] = []
    for provider_cls in _providers():
        res = provider_cls.detect(path, base_config)
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
        architecture_env_var = None
        if dep.var_name:
            version_var = dep.var_name
        if dep.architecture_var_name:
            architecture_env_var = dep.architecture_var_name
        vars = [f'"{dep.name}"']
        if version_var:
            vars.append(version_var)
        if architecture_env_var:
            vars.append(f"architecture={architecture_env_var}")
        lines.append(f"{alias} = dep({', '.join(vars)})")

    return "\n".join(lines), serve_vars, build_vars


def load_provider(
    path: Path, base_config: Config, use_provider: Optional[str] = None
) -> type[Provider]:
    provider_cls = None
    if use_provider:
        provider_cls = next(
            (p for p in _providers() if p.name().lower() == use_provider.lower()), None
        )
    if not provider_cls:
        provider_cls = detect_provider(path, base_config)
    return provider_cls


def load_provider_config(
    provider_cls: type[Provider],
    path: Path,
    base_config: Config,
    config: Optional[Union[dict, str]] = None,
) -> dict:
    provider_config = provider_cls.load_config(path, base_config)
    if config:
        if isinstance(config, str):
            config = json.loads(config)
        assert isinstance(config, dict), "Config must be a dictionary, got %s" % type(config)
        provider_config = provider_config.__class__.model_validate({**(provider_config.model_dump() | config)})
    return provider_config


def generate_shipit(path: Path, provider: Provider) -> str:
    default_serve_name = path.absolute().name

    # Collect parts
    plan = ProviderPlan(
        serve_name=provider.serve_name() or default_serve_name,
        provider=provider.name(),
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

    def format_command(k: str, v: str) -> str:
        return f'    "{k}": {v}'

    commands_lines = ",\n".join(
        [format_command(k, v) for k, v in plan.commands.items()]
    )
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

    if plan.mounts:
        for m in plan.mounts:
            out.append(f'{m.name} = mount("{m.name}")')
        out.append("")

    if plan.volumes:
        for v in plan.volumes:
            out.append(f'{v.var_name or v.name} = volume("{v.name}", {v.serve_path})')
        out.append("")

    if plan.services:
        for s in plan.services:
            out.append(
                f'{s.name} = service(\n  name="{s.name}",\n  provider="{s.provider}"\n)'
            )
        out.append("")

    if plan.declarations:
        out.append(plan.declarations)
        out.append("")

    out.append("serve(")
    out.append(f'  name="{plan.serve_name}",')
    out.append(f'  provider="{plan.provider}",')
    # If app is mounted for serve, set cwd to the app serve path
    if "app" in attach_serve_names:
        out.append('  cwd=app.serve_path,')
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
    if commands_lines:
        out.append("  commands = {")
        out.append(commands_lines)
        out.append("  },")
    else:
        out.append("  commands = {},")
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
