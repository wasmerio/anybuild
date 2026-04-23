# AGENTS.md

This file provides guidance to AI agents working in this repository.

## What is Shipit

Shipit is a Python CLI for building, running, and deploying projects
described with Starlark files. It can build locally, in Docker, or using
Wasmer, and supports examples for popular frameworks.

## Architecture

- **CLI**: `src/shipit/cli.py` implements the Typer commands `build`, `run`,
  `deploy`, and `auto`.
- **Build backends**: `LocalBuildBackend` runs steps on the host while
  `DockerBuildBackend` produces artifacts inside a container and exports them.
- **Runners**: `LocalRunner` executes the generated commands locally, and
  `WasmerRunner` packages artifacts and runs them with Wasmer.
- **Starlark runtime**: Build steps are defined in a `Shipit` file and executed
  through the `Ctx` interface.
- **Assets**: Templates and config snippets live under `src/shipit/assets`.
- **Examples**: Sample apps in `examples/` show how to use the tool.

## Bash commands

- `uv run shipit` – Generate the Shipit, build, and run the project.
- `uv run shipit generate` – Generate the `Shipit` file.
- `uv run shipit build` – Build the project defined by the `Shipit` file.
- `uv run shipit run` – Run the built project.
- `uv run shipit deploy` – Deploy the built Wasmer project.
- `uv run python` – Run Python (always prefer this over calling `python`
  directly).
- `uv run pytest` – Run the test suite (if tests exist).

## Testing

- Pytest is declared as a dev dependency in `pyproject.toml` under
  `[tool.uv].dev-dependencies`. Running `uv run pytest` will automatically use
  the project environment and install pytest if needed.
- No global installation or manual virtualenv activation is required.

## Code style

- Follow Python conventions (PEP 8) and existing patterns in the codebase.
- Use type hints where reasonable.
- Stick to the repo’s current Python baseline (3.13) and avoid shims for older
  versions; if a feature exists in 3.13, just use it without compatibility code.
- Avoid comments that simply restate code; explain *why*, not *what*.
- Keep imports grouped and sorted.
- Lines should be kept to 80 characters where possible.

## Workflow

- After making changes, run `uv run pytest` to verify nothing broke.
- Keep commits focused and write clear commit messages.
- Do not run tests or commands unrelated to your changes unless asked.

## File conventions

- Markdown files should wrap lines at 80 characters.
