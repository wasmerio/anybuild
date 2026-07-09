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


_STATICFILE_SERVE = ("//shipit/tools:staticfile.shipit", "staticfile_serve")

# Starlark stdlib entrypoints per provider: a (module, function) pair for the
# build and one for the serve. The generated Shipit file loads both and calls
# them in sequence, keeping the build/serve seam users compose on explicit —
# static-site builders pair their own build with the shared staticfile serve.
# The optional "provider" key is the deployment identity the generated file
# passes to the serve when it differs from the serve function's default.
STARLARK_ENTRYPOINTS: dict[str, dict] = {
    "python": {
        "build": ("//shipit/tools:python.shipit", "python_build"),
        "serve": ("//shipit/tools:python.shipit", "python_serve"),
    },
    "staticfile": {
        "build": ("//shipit/tools:staticfile.shipit", "staticfile_build"),
        "serve": _STATICFILE_SERVE,
    },
    "hugo": {
        "build": ("//shipit/tools:hugo.shipit", "hugo_build"),
        "serve": _STATICFILE_SERVE,
        "provider": "hugo",
    },
    "mkdocs": {
        "build": ("//shipit/tools:mkdocs.shipit", "mkdocs_build"),
        "serve": _STATICFILE_SERVE,
        "provider": "mkdocs",
    },
    "jekyll": {
        "build": ("//shipit/tools:jekyll.shipit", "jekyll_build"),
        "serve": _STATICFILE_SERVE,
        "provider": "jekyll",
    },
    "node-static": {
        "build": ("//shipit/tools:node_static.shipit", "nodestatic_build"),
        "serve": _STATICFILE_SERVE,
        "provider": "node-static",
    },
    "go": {
        "build": ("//shipit/tools:go.shipit", "go_build"),
        "serve": ("//shipit/tools:go.shipit", "go_serve"),
    },
    "php": {
        "build": ("//shipit/tools:php.shipit", "php_build"),
        "serve": ("//shipit/tools:php.shipit", "php_serve"),
    },
    "wordpress": {
        "build": ("//shipit/tools:wordpress.shipit", "wordpress_build"),
        "serve": ("//shipit/tools:wordpress.shipit", "wordpress_serve"),
    },
    "node": {
        "build": ("//shipit/tools:node.shipit", "node_build"),
        "serve": ("//shipit/tools:node.shipit", "node_serve"),
    },
    "laravel": {
        "build": ("//shipit/tools:laravel.shipit", "laravel_build"),
        "serve": ("//shipit/tools:laravel.shipit", "laravel_serve"),
    },
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
    entrypoint: dict,
    subdir: Optional[str] = None,
) -> str:
    build_module, build_function = entrypoint["build"]
    serve_module, serve_function = entrypoint["serve"]
    provider = entrypoint.get("provider")
    out: List[str] = []
    if build_module == serve_module:
        out.append(f'load("{build_module}", "{build_function}", "{serve_function}")')
    else:
        out.append(f'load("{build_module}", "{build_function}")')
        out.append(f'load("{serve_module}", "{serve_function}")')
    out.append("")
    if subdir:
        out.append(f"app_subdir = {json.dumps(subdir)}")
        out.append("")
    out.append(f"build = {build_function}(config)")
    out.append("")
    if provider:
        out.append(f'{serve_function}(config, build, provider = "{provider}")')
    else:
        out.append(f"{serve_function}(config, build)")
    out.append("")
    return "\n".join(out)
