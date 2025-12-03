from .base import Provider
from .hugo import HugoProvider
from .laravel import LaravelProvider
from .mkdocs import MkdocsProvider
from .node_static import NodeStaticProvider
from .wordpress import WordPressProvider
from .php import PhpProvider
from .python import PythonProvider
from .jekyll import JekyllProvider
from .staticfile import StaticFileProvider


def providers() -> list[type[Provider]]:
    # Order matters: more specific providers first
    return [
        LaravelProvider,
        HugoProvider,
        MkdocsProvider,
        PythonProvider,
        WordPressProvider,
        PhpProvider,
        NodeStaticProvider,
        JekyllProvider,
        StaticFileProvider,
    ]
