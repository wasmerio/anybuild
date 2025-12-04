# Shipit

Shipit is a CLI that automatically detects the type of project you are trying to run, builds it and runs it using [Starlark](https://starlark-lang.org/) definition files (called `Shipit`).

It can run builds locally, inside Docker, or through Wasmer, and bundles a one-command experience for common frameworks.

## Quick Start

To use shipit, you'll need to have [uv](https://docs.astral.sh/uv/) installed.

Install nothing globally; use `uvx shipit-cli` to run Shipit from anywhere.

```bash
uvx shipit-cli .
```

Running in `auto` mode will generate the `Shipit` file when needed, build the project, and can
also serve it. Shipit picks the safest builder automatically and falls back to
Docker or Wasmer when requested:

- `uvx shipit-cli . --wasmer` builds locally and serves inside Wasmer.
- `uvx shipit-cli . --docker` builds it with Docker (you can customize the docker client as well, eg: `--docker-client depot`).
- `uvx shipit-cli . --start` launches the app after building.

You can combine them as needed:

```
uvx shipit-cli . --start --wasmer --skip-prepare
```

## Commands

### Default `auto` mode

Full pipeline in one command. Combine flags such as `--regenerate` to rewrite
the `Shipit` file. Use
`--wasmer` to run with Wasmer, or `--wasmer-deploy` to deploy to Wasmer Edge.

### `generate`

```bash
uvx shipit-cli generate .
```

Create or refresh the `Shipit` file. Override build and run commands with
`--install-command`, `--build-command`, or `--start-command`. Pick a exlicit provider
with `--use-provider`.

### `plan`

```bash
uvx shipit-cli plan --out plan.json
```

 Evaluate the project and emit config, derived commands, and required
services without building. Helpful for CI checks or debugging configuration.

### `build`

```bash
uvx shipit-cli build
```

Run the build steps defined in `Shipit`. Append `--wasmer` to execute inside
Wasmer, `--docker` to use Docker builds.

### `serve`

```bash
uvx shipit-cli serve
```

Execute the start command for the project. Combine with `--wasmer` for WebAssembly execution, or `--wasmer-deploy` to deploy to Wasmer Edge.

## Supported Technologies

Shipit works with three execution environments:

- Local builder for fast, host-native builds.
- Docker builder when container isolation is required.
- Wasmer runner for portable WebAssembly packaging and deployment.

## Development

Clone the repository and use the `uv` project environment.

```bash
uv run shipit . --start
```

Use any other subcommand during development by prefixing with `uv run shipit`,
for example `uv run shipit build . --wasmer`. This keeps changes local while
matching the published CLI behaviour.

### Tests

Run the test suite with:

```bash
uv run pytest
```

You can run the e2e tests in parallel (`-n 8`) with:

```bash
uv run pytest -m e2e -v "tests/test_e2e.py" -s -n 8
```

The e2e tests will:
* Build the project (locally, or with docker)
* Run the project (locally or with Wasmer)
* Test that the project output (via http requests) is the correct one
