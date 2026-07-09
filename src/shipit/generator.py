import json
from pathlib import Path
from typing import List, Optional, Union

from shipit.providers.base import (
    Provider,
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
) -> Config:
    provider_config = provider_cls.load_config(path, base_config)
    if config:
        if isinstance(config, str):
            config = json.loads(config)
        assert isinstance(config, dict), "Config must be a dictionary, got %s" % type(config)
        provider_config = provider_config.__class__.model_validate({**(provider_config.model_dump() | config)})
    if not provider_config.name:
        provider_config.name = path.absolute().name
    return provider_config


# Starlark stdlib entrypoint per provider: provider name -> (module, function).
# The generated Shipit file is a two-liner loading the bundled library.
STARLARK_ENTRYPOINTS: dict[str, tuple[str, str]] = {
    "python": ("//shipit/tools:python.shipit", "python_build_and_serve"),
    "staticfile": ("//shipit/tools:staticfile.shipit", "staticfile_build_and_serve"),
    "hugo": ("//shipit/tools:hugo.shipit", "hugo_build_and_serve"),
    "mkdocs": ("//shipit/tools:mkdocs.shipit", "mkdocs_build_and_serve"),
    "go": ("//shipit/tools:go.shipit", "go_build_and_serve"),
    "jekyll": ("//shipit/tools:jekyll.shipit", "jekyll_build_and_serve"),
    "php": ("//shipit/tools:php.shipit", "php_build_and_serve"),
    "wordpress": ("//shipit/tools:wordpress.shipit", "wordpress_build_and_serve"),
    "node": ("//shipit/tools:node.shipit", "node_build_and_serve"),
    "node-static": ("//shipit/tools:node_static.shipit", "nodestatic_build_and_serve"),
    "laravel": ("//shipit/tools:laravel.shipit", "laravel_build_and_serve"),
}


def generate_shipit(
    path: Path,
    provider: Provider,
    subdir: Optional[str] = None,
) -> str:
    entrypoint = STARLARK_ENTRYPOINTS.get(provider.name())
    if not entrypoint:
        raise Exception(
            f"No Starlark provider entrypoint registered for {provider.name()!r}"
        )
    return generate_shipit_loader(entrypoint, subdir=subdir)


def generate_shipit_loader(
    entrypoint: tuple[str, str],
    subdir: Optional[str] = None,
) -> str:
    module, function = entrypoint
    out: List[str] = [f'load("{module}", "{function}")', ""]
    if subdir:
        out.append(f"app_subdir = {json.dumps(subdir)}")
        out.append("")
    out.append(f"{function}(config)")
    out.append("")
    return "\n".join(out)
