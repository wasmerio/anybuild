# AGENTS.md

This file provides guidance to AI agents working in this repository.

## What is Anybuild

Anybuild is a CLI for building, running, and deploying projects
described with Starlark files. It can build locally, in Docker, or using
Wasmer, and supports examples for popular frameworks.

## Architecture

- **CLI**: `crates/anybuild-cli` implements the commands `build`, `run`,
  `deploy`, and `auto`.
- **Build backends**: `LocalBuildBackend` runs steps on the host while
  `DockerBuildBackend` produces artifacts inside a container and exports them.
- **Runners**: `LocalRunner` executes the generated commands locally, and
  `WasmerRunner` packages artifacts and runs them with Wasmer.
- **Starlark runtime**: Build steps are defined in an `Anybuild` file and executed
  through the `Ctx` interface.
- **Assets**: Templates and config snippets live under `resources/assets`.
- **Examples**: Sample apps in `examples/` show how to use the tool.

## Bash commands

- `cargo run -- .` – Generate the Anybuild and build the project.
- `cargo run -- generate` – Generate the `Anybuild` file.
- `cargo run -- build` – Build the project defined by the `Anybuild` file.
- `cargo run -- run` – Run the built project.
- `cargo run -- deploy` – Deploy the built Wasmer project.
- `cargo test --workspace` – Run the Rust test suite.
- `scripts/verify_rust.sh` – Run formatting, build, Clippy, tests, and smoke
  gates.

## Testing

- `cargo test --workspace` runs the unit, integration, snapshot, fixture, and
  generated-file tests. End-to-end tests are ignored by default.
- `scripts/verify_rust.sh --e2e` also runs the Wasmer end-to-end slice.

## Code style

- Follow Rust conventions and existing patterns in the workspace.
- Run `cargo fmt --all` after editing Rust code.
- Keep the workspace clean under `cargo clippy --workspace --all-targets`.
- Avoid comments that simply restate code; explain *why*, not *what*.
- Keep imports grouped and sorted.
- Lines should be kept to 80 characters where possible.

## Workflow

- After making changes, run `scripts/verify_rust.sh` to verify nothing broke.
- Keep commits focused and write clear commit messages.
- Do not run tests or commands unrelated to your changes unless asked.

## File conventions

- Markdown files should wrap lines at 80 characters.
