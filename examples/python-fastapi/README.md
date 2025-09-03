# Run it with Wasmer

First, install dependencies:

```
pip install -r wasmer-requirements.txt --target wasix-site-packages --platform wasix_wasm32 --only-binary=:all: --python-version=3.13 --compile
```

And then (*the first time it runs it will take about a few minutes to compile Python*):
```
wasmer run . --registry=wasmer.wtf
```

> Note: you'll need Wasmer 6.1.0-rc.2 to run this. . Check the [installation instructions here](https://github.com/wasmerio/wasmer/releases/tag/v6.1.0-rc.2).

All Native Python packages that are available right now in Wasmer can be found here:
https://pythonindex.wasix.org/

# How to install dependencies

This process is going to be streamlined using wasmer's autobuild, so you just need to upload a zip with the source and `requirements.txt`. Until that happens, please do:

## If you have `pyproject.toml`

> Note: Right now, the process is not ideal as it's using `uvx pip` instead of `uv pip`, but we need to create a PR to `uv` to support `wasix_wasm32` target first.

```
uv pip compile pyproject.toml --python-version=3.13 --universal --extra-index-url https://pythonindex.wasix.org/simple --index-url=https://pypi.org/simple --emit-index-url --only-binary :all: -o wasmer-requirements.txt
uvx pip install -r wasmer-requirements.txt --target wasix-site-packages --platform wasix_wasm32 --only-binary=:all: --python-version=3.13 --compile
```

## If you have `requirements.txt`

```
pip install -r requirements.txt --target wasix-site-packages --platform wasix_wasm32 --only-binary=:all: --python-version=3.13 --index-url https://pypi.org/simple
--extra-index-url https://pythonindex.wasix.org/simple --compile
```
