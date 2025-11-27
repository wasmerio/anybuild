# Shipit Architecture

Shipit is a Python CLI that detects a project type, generates a Starlark
`Shipit` file, builds the project, and optionally serves or deploys it. This
document explains the main components, how detection and generation work, and
how builds are executed across the local, Docker, and Wasmer backends.

## CLI entrypoints and control flow

- `shipit auto` is the default entrypoint. It regenerates the `Shipit` file if
  needed, runs `build`, and optionally runs `serve` or Wasmer deploy steps.
- `shipit generate` detects a provider and writes a `Shipit` file. Flags let
  callers override install/build/start commands or force a provider.
- `shipit plan` evaluates the `Shipit` file without building, returning JSON
  with provider, derived commands, required config, and services.
- `shipit build` evaluates the `Shipit` file, resolves env vars from `.env`
  (and `.env.<env>` when `--env-name` is set), and runs the build pipeline.
- `shipit serve` runs the `start` command (or deploys) using the selected
  backend.

`main()` in `src/shipit/cli.py` routes to `auto` when invoked without a
subcommand so `shipit .` performs the full pipeline by default.

## Shipit Starlark runtime

`evaluate_shipit` (cli.py) loads the `Shipit` file with the `starlark` runtime,
registering functions on a `Ctx` instance:

- `dep`, `service`, `mount`, `volume` declare packages, services, mounts, and
  volumes. These return reference strings later resolved by `Ctx.get_ref`.
- `run`, `copy`, `workdir`, `path`, `env`, `use` create build steps.
- `serve` assembles a single `Serve` definition (enforced to be 1 per file).
- `getenv` reads environment variables and records which names were accessed.

The runtime uses the extended dialect with f-strings enabled. `Ctx` stores
packages, steps, builds, serves, mounts, volumes, and services. `serve(...)`
resolves references into dataclasses (`Serve`, `Build`, `Step`) consumed by
builders.

### Step types

- `RunStep`: shell command, optional input/output hints, and an optional group
  tag used for metadata (install/build).
- `CopyStep`: copy from source to target, with optional ignore globs and an
  optional `assets` base (for files under `src/shipit/assets`). HTTP/HTTPS
  sources trigger a download.
- `WorkdirStep`: change working directory.
- `EnvStep`: set environment variables for subsequent steps.
- `PathStep`: prepend to `PATH`.
- `UseStep`: declare tool dependencies to install (backend-specific).

### Mounts, volumes, services

`mount(name)` defines paired build/serve paths derived from the builder. Common
mounts are `app` (source) and `venv`/`local_venv` for Python. `volume(name,
serve)` describes runtime volumes (e.g., WordPress content). `service(name,
provider)` attaches managed services such as MySQL or Postgres to the serve
definition.

## Shipit file structure

Generated files follow a consistent pattern:

- Dependency declarations via `dep(...)`, often gated by `getenv` to allow
  version overrides (`SHIPIT_*` variables).
- Mount and volume creation.
- Optional service declarations.
- `PORT = getenv("PORT") or "8080"` defaulting.
- Provider-specific declarations (temporary variables, paths).
- A single `serve(...)` block with:
  - `provider`: provider name.
  - `build`: ordered steps (often starting with `use(...)`).
  - `deps`: dependency variables to include in the serve environment.
  - `prepare`: optional steps run after build but before packaging/serve.
  - `commands`: `start` plus optional `after_deploy` etc.
  - `cwd`, `env`, `mounts`, `volumes`, `services` as needed.

`generate_shipit` injects a `use(...)` step automatically when build-time
dependencies exist and no explicit `use` step is present.

## Provider model

Providers encapsulate language/framework knowledge. Each implements the
`Provider` protocol (`src/shipit/providers/base.py`) and produces a
`ProviderPlan` consumed by `generate_shipit`. Key pieces:

- `detect(path, custom_commands) -> DetectResult | None`: scoring heuristic to
  match a project. Higher scores win; ties are resolved by declaration order in
  `registry.py`.
- `dependencies()`: list of `DependencySpec` objects describing tool names,
  env-variable overrides, default versions, architecture variants, and whether
  the dependency is needed during build and/or serve.
- `build_steps()`: list of Starlark step expressions (strings) referencing
  mounts and helper variables.
- `prepare_steps()`: optional list of Starlark run steps executed after build
  (e.g., Python bytecode precompilation).
- `commands()`: `start` and optional lifecycle commands.
- `mounts()`, `volumes()`, `services()`: attach mount specs, runtime volumes,
  and managed services.
- `env()`: extra environment variables to surface in `serve(...)`.

### Detection flow

`detect_provider` iterates over the provider registry, collecting non-null
`DetectResult`s and selecting the highest score. `CustomCommands` (derived from
CLI flags or a Procfile) influence detection for cases like `start` commands
(`static-web-server`, `php`, `uvicorn`, `mkdocs`, `jekyll`).

### Provider registry (order = priority)

1. `LaravelProvider`: requires `artisan` and `composer.json`.
2. `HugoProvider`: config files (`hugo.*` or generic `config.*` with content).
3. `MkdocsProvider`: `mkdocs.yml`/`mkdocs.yaml` or `mkdocs` build command.
4. `PythonProvider`: `pyproject.toml`/`requirements.txt` or Python-esque start
   commands or known `main.py`/`app.py` patterns.
5. `WordPressProvider`: `wp-content`, `index.php`, `wp-load.php`.
6. `PhpProvider`: `composer.json` plus `public/app/index.php` or PHP start
   commands.
7. `NodeStaticProvider`: `package.json` with static-site dependencies
   (Astro/Vite/Next/Gatsby/Svelte/Docusaurus/Remix/Nuxt) and lockfiles.
8. `JekyllProvider`: `_config.(yml|yaml)` optionally with `Gemfile`.
9. `StaticFileProvider`: `Staticfile`, or `index.html` without other stacks, or
   a `static-web-server` start command.

### Provider behavior (high level)

- **StaticFileProvider**: copies static assets, depends on `static-web-server`,
  mounts `app`, and starts `static-web-server --root=<serve>`.
- **NodeStaticProvider**: chooses package manager from lockfile, infers static
  generator to pick build command and output dir, installs Node + package
  manager + `static-web-server`, runs install/build, and copies the built
  directory into the `app` mount.
- **HugoProvider**: installs Hugo, builds into `temp` then copies to `app`
  serve mount.
- **MkdocsProvider**: leverages `PythonProvider` (build-only) with extra
  dependency `mkdocs`, then runs `uv run mkdocs build` into the serve mount.
- **JekyllProvider**: installs Ruby (and `bundle install` when `Gemfile` is
  present), runs `jekyll build` into `app`.
- **PythonProvider**: detects frameworks (Django, FastAPI, Flask, Streamlit,
  FastHTML, MCP), ASGI/WSGI servers, databases (MySQL/Postgres), and auxiliary
  tools (ffmpeg, pandoc). Declares `python` and `uv` dependencies (plus extras
  like `uvicorn`, `pandoc`, `ffmpeg`). Build steps create venv mounts, run `uv
  sync/add`, optionally compile cross-platform wheels, and copy the source.
  Prepare steps optionally precompile bytecode. Commands auto-generate `start`
  for each framework (e.g., `uvicorn`, Django `runserver`) and `after_deploy`
  migrations for Django. Mounts include `app`, `venv`, and `local_venv`.
- **PhpProvider**: installs PHP (with architecture override support) and
  Composer when present, copies a `php.ini` asset if none is provided, runs
  `composer install`, and serves via `php -S` in the appropriate directory.
- **LaravelProvider**: PHP + Composer + `pie` + `pnpm`, runs Laravel caches in
  prepare, and provides an `after_deploy` migration command.
- **WordPressProvider**: extends PHP; downloads `wp-cli`, seeds `wp-config.php`
  when missing, attaches a `wp-content` volume, adds MySQL service, and sets an
  `after_deploy` script.

## Generation pipeline

`generate_shipit` creates a `ProviderPlan` and materializes it as Starlark:

1. Detect provider (or honor `--use-provider`).
2. Collect defaults: serve name (directory name by default), provider name,
   mounts, volumes, dependencies, build steps, prepare steps, commands,
   services, env, and declarations.
3. Emit dependency declarations with version/architecture variables derived
   from `DependencySpec` env vars (e.g., `SHIPIT_NODE_VERSION`).
4. Serialize mounts/volumes/services into variables.
5. Inject `PORT` defaulting logic and any provider declarations.
6. Render the `serve(...)` block with build, deps, commands, env, services,
   mounts, volumes, and prepare steps.

`tests/test_generate_shipit_examples.py` asserts that generation matches the
checked-in `Shipit` files under `examples/`, ensuring stability of provider
plans and formatting.

## Build pipeline

`build` (cli.py) orchestrates execution:

1. Resolve `Shipit` path (default `./Shipit`).
2. Select backend: Docker when `--docker` (or explicit client), otherwise
   local. Wrap with `WasmerBuilder` when `--wasmer` or deploy flags are set.
3. Evaluate the `Shipit` file to obtain `Serve` and capture `getenv` usage.
4. Merge environment variables from `.env` and `.env.<env_name>` into
   `serve.env`.
5. Run `builder.build(env, mounts, steps)`.
6. Optionally run `builder.build_prepare(serve)` and `builder.prepare(...)`
   when prepare steps exist (skipped with `--skip-prepare`).
7. Generate serve artifacts via `builder.build_serve(serve)` and finalize the
   build via `builder.finalize_build(serve)`.

`skip_docker_if_safe_build` reroutes to the local builder when the build has no
`RunStep` (copy-only builds) even if Docker was requested, trading isolation for
speed.

## Builders

### LocalBuilder

- Output lives under `.shipit/local/`. `build_path` defaults to
  `.shipit/local/build`.
- Executes steps directly on the host:
  - `RunStep` uses `bash -c` with the working directory, copying declared
    inputs into the build directory first.
  - `CopyStep` copies from source or assets (filtering ignore globs and always
    ignoring `.shipit`/`Shipit`).
  - `EnvStep` and `PathStep` mutate the running `env` dict.
  - `WorkdirStep` updates the working directory, creating it if needed.
  - `UseStep` logs dependencies but does not install (assumes tools already
    available).
- Persists `PATH` in `.shipit/local/.path` for use by serve scripts.
- `build_prepare` writes a `prepare.sh` script under `.shipit/local/prepare`
  and prints it.
- `build_serve` writes runnable scripts under `.shipit/local/serve/bin/<cmd>`,
  injecting env vars and PATH, and prints the scripts.
- `run_serve_command` executes the generated script.

### DockerBuilder

- Writes under `.shipit/docker/`, generating a multi-stage Dockerfile:
  - Base image `debian:trixie-slim` plus build tooling and `mise` for tool
    installation.
  - Creates mount directories inside the image.
  - Translates steps into Dockerfile instructions (`RUN`, `COPY`, `ADD`, `ENV`)
    and maps `UseStep` dependencies to `mise use --global` invocations. Special
    cases include `pie` (manual install) and `static-web-server`.
  - Copies build artifacts into a scratch final image, respecting mounts.
  - Generates a `.dockerignore` that excludes `.shipit` and `Shipit`.
- `finalize_build` writes the Dockerfile, image name, and triggers `docker build
  -o .shipit/docker/out --platform linux/amd64 ...`.
- `build_serve` creates `/shipit/serve/bin/<cmd>` files inside the Dockerfile
  so commands can be executed via `docker run`.
- `run_serve_command` runs the image with `docker run -p 80:80 --rm <cmd>`.

### WasmerBuilder

- Wraps another builder (local or Docker) but adjusts mount paths for Wasmer
  (`/app`, `/opt/<name>` for serve).
- Maintains a Wasmer workspace at `.shipit/wasmer`.
- Maps dependencies to Wasmer packages via `mapper` (e.g., `python` ->
  `python/python@...`, `static-web-server` -> `wasmer/static-web-server@...`).
  Adds `bash` automatically when prepare steps exist.
- `build_serve` emits:
  - `wasmer.toml` with package dependencies, command entries, cwd, env, and fs
    mapping from serve mounts to inner builder output paths.
  - `app.yaml` for Wasmer deployment, including capabilities (database),
    volumes, scaling tweaks for PHP, and a `post-deployment` job when
    `after_deploy` exists.
- `build_prepare` writes a `prepare.sh` with env exports and prepare commands.
- `prepare` runs the prepare script inside Wasmer with `wasmer run` and a
  mapped `/prepare` directory.
- `run_serve_command` executes `wasmer run .shipit/wasmer --command=<cmd>`,
  optionally against a custom registry/token.
- `deploy_config` and `deploy` build/publish Wasmer packages and emit metadata.

## Providers, mounts, and assets

- Mount semantics differ per builder:
  - Local: `app` -> `.shipit/local/build/app`, `venv` -> `.shipit/local/build`
    `/venv`.
  - Docker: `app` -> `/app` inside the image, surfaced to `.shipit/docker/out`.
  - Wasmer: serve mounts are `/app` and `/opt/<name>`, mapped to inner builder
    outputs for packaging.
- Assets under `src/shipit/assets` supply defaults (e.g., `php/php.ini`,
  `wordpress/wp-config.php`, `wordpress/install.sh`).
- Volumes: WordPress exposes a `wp-content` volume; other providers currently
  return no volumes.
- Services: Python can declare `database` (MySQL/Postgres) based on detected
  dependencies; WordPress declares MySQL; other providers omit services.

## Custom commands and Procfile integration

`CustomCommands` collect overrides from CLI flags or a Procfile (`Procfile` is
parsed via `procfile.py`). These values feed provider detection and command
generation (e.g., overriding `start`). The `plan` command also attempts to
derive `install`/`build` commands by grouping `RunStep` entries via their
`group` field.

## Observability and debugging

- Builders stream stdout/stderr via `write_stdout`/`write_stderr`.
- Generated Dockerfiles, Wasmer manifests, and prepare scripts are printed with
  syntax highlighting (Rich) for transparency.
- `SHIPIT_DEBUG=true` re-raises exceptions after pretty-printing in `main()`.

## Key logic flows (end-to-end)

1. **Detection & generation**: detect provider -> build `ProviderPlan` ->
   render `Shipit` file with dependencies, mounts, build/prepare steps, and
   commands.
2. **Evaluation**: parse Starlark -> build `Ctx` -> materialize `Serve` and
   supporting objects -> capture `getenv` usage.
3. **Build**: run steps with selected builder -> optional prepare -> emit serve
   artifacts (scripts, Dockerfile, Wasmer manifests) -> finalize/build images
   when applicable.
4. **Serve/Deploy**: execute the `start` command via local script, Docker
   container, or `wasmer run`; optionally package/deploy via Wasmer.

This architecture keeps provider logic declarative (Starlark templates) while
allowing multiple backend implementations to share the same plan and produce
consistent serve artifacts.
