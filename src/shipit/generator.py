from __future__ import annotations

from pathlib import Path
from typing import Dict, List, Optional

from shipit.providers.base import DependencySpec, Provider, ProviderPlan, DetectResult
from shipit.providers.registry import providers as registry_providers


def _providers() -> list[Provider]:
    # Load providers from modular registry
    return registry_providers()


def detect_provider(path: Path) -> Provider:
    matches: list[tuple[Provider, DetectResult]] = []
    for p in _providers():
        res = p.detect(path)
        if res:
            matches.append((p, res))
    if not matches:
        # Default to static site as the safest fallback
        from shipit.providers.staticfile import StaticFileProvider

        return StaticFileProvider()
    # Highest score wins; tie-breaker by order
    matches.sort(key=lambda x: x[1].score, reverse=True)
    return matches[0][0]


def _sanitize_alias(name: str) -> str:
    # Keep it predictable and valid in Starlark: letters, numbers, underscore
    # Remove dashes to keep prior style (e.g., staticwebserver)
    allowed = [c if c.isalnum() or c == "_" else "" for c in name]
    alias = "".join(allowed)
    return alias.replace("-", "")


def _emit_dependency_block(deps: List[DependencySpec]) -> tuple[str, List[str]]:
    lines: List[str] = []
    var_names: List[str] = []
    for dep in deps:
        alias = dep.alias or _sanitize_alias(dep.name)
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
        var_names.append(alias)
    return "\n".join(lines), var_names


def _render_assets(assets: Optional[Dict[str, str]]) -> Optional[str]:
    if not assets:
        return None
    inner = ",\n".join([f'    "{k}": {v}' for k, v in assets.items()])
    return f"{{\n{inner}\n  }}"


def generate_shipit(path: Path) -> str:
    provider = detect_provider(path)
    provider.initialize(path)

    # Collect parts
    plan = ProviderPlan(
        serve_name=provider.serve_name(path),
        provider=provider.provider_kind(path),
        build_dependencies=provider.build_dependencies(path),
        serve_dependencies=provider.serve_dependencies(path),
        build_steps=provider.build_steps(path),
        prepare=provider.prepare_script(path),
        commands=provider.commands(path),
        assets=provider.assets(path),
        mounts=provider.mounts(path),
    )

    # Build dependency variables
    build_dep_block, _ = _emit_dependency_block(plan.build_dependencies)
    serve_dep_block, serve_dep_vars = _emit_dependency_block(plan.serve_dependencies)

    # Compose serve(...) body
    build_steps_block = ",\n".join([f"    {s}" for s in plan.build_steps])
    deps_array = ", ".join(serve_dep_vars)
    commands_lines = ",\n".join([f'    "{k}": {v}' for k, v in plan.commands.items()])
    assets_block = _render_assets(plan.assets)
    mounts_block = None
    if plan.mounts:
        mounts_block = ",\n".join([f"    {k}: {v}" for k, v in plan.mounts.items()])

    out: List[str] = []
    if build_dep_block:
        out.append(build_dep_block)
        out.append("")
    if serve_dep_block:
        out.append(serve_dep_block)
        out.append("")
    out.append("serve(")
    out.append(f'  name="{plan.serve_name}",')
    out.append(f'  provider="{plan.provider}",')
    out.append("  build=[")
    out.append(build_steps_block)
    out.append("  ],")
    if assets_block:
        out.append("  assets=" + assets_block + ",")
    out.append(f"  deps=[{deps_array}],")
    if plan.prepare:
        out.append("  prepare=\"\"\"")
        out.append(plan.prepare.rstrip("\n"))
        out.append("\"\"\",")
    out.append("  commands = {")
    out.append(commands_lines)
    out.append("  },")
    if mounts_block:
        out.append("  mounts={")
        out.append(mounts_block)
        out.append("  },")
    out.append(")")
    out.append("")
    return "\n".join(out)

