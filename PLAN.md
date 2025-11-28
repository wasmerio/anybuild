# Rust rewrite plan

Step-by-step plan to port Shipit from Python to Rust, using `starlark` for both
evaluation and AST-based Shipit file generation.

1. [x] Project scaffolding and dependencies
   - Expand `rust/Cargo.toml` with crates: `starlark`, `clap`/`typer`-like CLI,
     `anyhow`, `thiserror`, `serde`/`serde_json`, `tokio` (if async needed),
     `tracing` or `log`, `dirs`, `which`, `camino`/`path` utilities, `regex`.
   - Organize modules: `cli`, `context` (Ctx/Serve), `model` (steps/mounts),
     `starlark_runtime`, `generator`, `providers`, `builders`, `assets`,
     `env`, `procfile`, `detect`, `util`.
   - Decide binary layout: library crate plus binary target `shipit`.

2. [x] Domain models and shared types
   - Recreate dataclasses as structs/enums: `Step` variants (Run, Copy,
     Workdir, Env, Path, Use), `Serve`, `Build`, `Mount`, `Volume`, `Service`,
     `DependencySpec`, `ProviderPlan`, `DetectResult`, `CustomCommands`.
   - Implement serde support for JSON outputs (plan command) and cloning where
     helpful. Keep line-length friendly Display/debug helpers.
   - Centralize constants for default paths, mount names, env prefixes.

3. [x] Traits and abstractions
   - Define `Provider` trait with `detect`, `plan` (or `build_plan`), and
     accessors for dependencies, steps, commands, mounts, volumes, services,
     env. Provide registry management with ordered priority.
   - Define `Builder` trait with `build`, `build_prepare`, `prepare`,
     `build_serve`, `finalize_build`, `run_serve_command`. Implement shared
     interfaces for writing artifacts and streaming logs.
   - Add supporting traits/helpers for dependency mapping and command
     generation to keep providers declarative.

4. [ ] Starlark integration
   - Implement runtime to evaluate `Shipit` files with `starlark::Evaluator`,
     registering builtins (`dep`, `service`, `mount`, `volume`, `run`, `copy`,
     `workdir`, `path`, `env`, `use`, `serve`, `getenv`) against a Rust `Ctx`
     that accumulates steps and metadata, including getenv tracking. **Done:
     runtime implemented with typed NoneOr/AllocList handling and shipit
     dialect.**
   - Ensure dialect matches Python version (f-string enabled); add tests that
     parse existing example `Shipit` files.
   - Build AST-based generator: construct Starlark programmatically instead of
     string concatenation. **In progress: custom minimal Starlark AST + manual
     renderer emitting staticfile Shipit end-to-end; tolerant parsing of plan
     build steps into typed nodes; broader providers and richer expressions
     still pending. Default PORT now emitted via `getenv("PORT") or "8080"`.**

5. [ ] Core control flows
   - Port detection pipeline `detect_provider` with scoring/tiebreak logic and
     custom command influence. **Started: registry-based detection helper and
     CLI wiring for generate/auto stubs.**
   - Implement generation pipeline: detect provider, assemble `ProviderPlan`,
     derive dependency vars, mounts, env, commands, inject `PORT` default, and
     emit Shipit via AST generator. **In progress: generate now emits
     serde_starlark-based Shipit for staticfile; broader providers still TODO.**
   - Implement evaluation/build pipeline: resolve Shipit path, select backend
     (local/docker/wasmer) with `skip_docker_if_safe_build`, merge `.env` +
     `.env.<env>` vars, run builder phases, and capture serve scripts/artifacts.
     **In progress: build flow now evaluates Shipit via Starlark and drives the
     local builder; staticfile end-to-end works.**

6. [ ] Builder implementations
   - LocalBuilder: host execution, file copying/ignores, PATH/env tracking,
     prepare and serve script generation mirroring Python behavior. **In
     progress: staticfile build/copy/serve script works; build_serve now copies
     build output into serve mount.**
   - DockerBuilder: Dockerfile synthesis, `.dockerignore`, multi-stage layout,
     handling `UseStep` via mise installs and special cases (pie/static-web-
     server), final image output and serve script embedding.
   - WasmerBuilder: wrap inner builder, adjust mount paths, map dependencies to
     Wasmer packages, emit `wasmer.toml` and `app.yaml`, handle prepare run and
     deploy metadata. **In progress: static-web-server + bash mapping via
     wasmer-config manifest/app.yaml generation; CLI can build with `--wasmer`
     using LocalBuilder as inner backend; cwd now resolved to /app in manifests;
     argv placeholders for ${PORT:-8080} are rewritten to the concrete port.**

7. [ ] Provider ports (incremental)
   - Start with low-risk: StaticFile, NodeStatic, Hugo, Mkdocs, Jekyll.
   - Port PythonProvider (framework detection, deps, mounts, prepare steps),
     then Php, Laravel, WordPress with services/volumes and asset handling.
   - Keep detection heuristics, dependency overrides, and commands aligned with
     Python behavior; add targeted tests/golden plans.

8. [ ] CLI surface and commands
   - Implement CLI parity: `auto`, `generate`, `plan`, `build`, `serve` (plus
     deploy flags if applicable). Set default routing so `shipit .` runs auto.
     **In progress: generate/build/serve wired to builder pipeline; auto handles
     regenerate/start flags for staticfile/local/wasmer; root-level `--wasmer`
     / `--start` flags now map to auto; docker still TODO.**
   - Wire logging/verbosity, error handling, and debug mode re-raise behavior.
   - Provide `plan` JSON output and human-readable summaries.

9. [ ] Testing and validation
   - Unit tests for starlark runtime, provider detection, plan generation, and
     builder translation pieces (Dockerfile snippets, Wasmer manifests).
   - Golden tests for generating Shipit files for `examples/`.
   - Integration tests for CLI flows where feasible (tmp dirs, fixtures).

10. [ ] Migration/stabilization
    - Keep Python version runnable during transition; gate Rust binary via
      feature flag or separate command until parity is proven.
    - Plan final switch-over (CLI entrypoint) after providers/builders reach
      parity, update docs, and clean up deprecated Python code.
