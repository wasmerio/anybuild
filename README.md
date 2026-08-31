# Anybuild

Anybuild is a Rust CLI that automatically detects the type of project you are
trying to run, builds it, and runs it using
[Starlark](https://starlark-lang.org/) definition files (called `Anybuild`).

It can run builds locally, inside Docker, or through Wasmer, and bundles a one-command experience for common frameworks.

## Quick Start

Install the latest release on macOS or Linux:

```bash
curl -fsSL https://anybuild.run/install | sh
. "$HOME/.anybuild/env"
anybuild .
```

Running in `auto` mode will generate the `Anybuild` file when needed, build the
project, and can also run it. Anybuild picks the safest builder automatically
and falls back to Docker or Wasmer when requested:

- `anybuild . --runner=wasmer` builds locally and runs inside Wasmer.
- `anybuild . --runner=docker` packages the build as a deployable Docker
  image and runs commands in containers.
- `anybuild . --runner=lambda` packages one AWS Lambda artifact, selecting a
  managed-runtime ZIP or container image from the serving dependencies.
- `anybuild . --builder=docker` builds it with Docker (you can customize the
  docker client as well, eg: `--docker-client depot`).
- `anybuild . --start` launches the app after building.
- `anybuild . --platform=wasmer` builds a Wasmer runtime artifact and deploys
  it to Wasmer.
- `anybuild . --platform=fly --fly-app=my-app` builds a Docker runtime
  artifact and deploys it to Fly.io.
- `anybuild . --platform=aws-lambda --aws-function=my-function` builds a
  Lambda artifact and deploys it using a managed runtime or a container image.

`--wasmer` remains shorthand for `--runner=wasmer`. `--docker` selects both
the Docker builder and Docker runner unless another runner is selected. For
example, `--wasmer --docker` uses the Docker builder and Wasmer runner.

The builder and runner are independent. The builder controls where build
steps execute; the runner produces the runtime artifact and, where supported,
can execute it. The deployment platform publishes a compatible runtime
artifact. For example, deployment images can be produced from either local or
Docker-built outputs:

```bash
anybuild build . --builder=local --runner=docker
anybuild build . --builder=docker --runner=docker
```

Python virtual environments produced by the local builder are host-specific.
Use `--builder=docker --runner=docker` for Python deployment images.

The resulting image is tagged with the normalized application name. Its
generated Dockerfile and image name are stored under
`.anybuild/runner/docker/` for later deployment.

You can combine them as needed:

```
anybuild . --start --runner=wasmer --skip-prepare
```

## Commands

### Default `auto` mode

Full pipeline in one command. Combine flags such as `--regenerate` to rewrite
the `Anybuild` file. Use `--runner=wasmer` to run with Wasmer.

### `generate`

```bash
anybuild generate .
```

Create or refresh the `Anybuild` file. Override build and run commands with
`--install-command`, `--build-command`, or `--start-command`. Pick an explicit
provider with `--use-provider`.

### `plan`

```bash
anybuild plan --out plan.json
```

 Evaluate the project and emit config, derived commands, and required
services without building. Helpful for CI checks or debugging configuration.

### `build`

```bash
anybuild build
```

Run the build steps defined in `Anybuild`. Use `--runner=wasmer` to prepare for
Wasmer or `--builder=docker` to use Docker builds.

### `run`

```bash
anybuild run
```

Run explicit commands for the project. Use `--start` to run the start
command, or pass one or more `--command` values. Use `--runner=wasmer` for
WebAssembly execution.

### `deploy`

```bash
anybuild deploy
```

Deploy a runtime artifact to a deployment platform. Wasmer remains the
default:

```bash
anybuild deploy --platform=wasmer
```

The artifact must first be generated with `anybuild build --runner=wasmer`.
Use `--wasmer-deploy-config` to write deployment metadata instead of
publishing.

Fly.io consumes the Docker runtime artifact:

```bash
anybuild build --runner=docker
anybuild deploy --platform=fly --fly-app=my-app
```

The Fly.io app must already exist. Anybuild uses the project's `fly.toml` when
present. Otherwise it generates a minimal configuration from `--fly-app` and
the Docker runtime port. Use `--fly-config`, `--fly-bin`, or `--fly-token` to
override the configuration, CLI binary, or credentials. The `FLY_API_TOKEN`
environment variable and flyctl's configured credentials also work.

The Lambda runner produces one AWS Lambda artifact. When the service runtime
dependencies are Python or Node.js, with optional Bash, it packages the
Docker-built Linux artifacts as a `.zip` for the matching AWS-managed runtime.
Other runtime dependencies produce a container image. Both paths include the
AWS Lambda Web Adapter, so regular HTTP applications can run on Lambda without
a Lambda-specific handler:

```bash
anybuild build --builder=docker --runner=lambda
anybuild deploy --platform=aws-lambda \
  --aws-function=my-function \
  --aws-region=us-west-2
```

`anybuild auto --platform=aws-lambda` selects the Docker builder and Lambda
runner unless either is explicitly set. This makes Python and Node.js
dependencies portable to Lambda's Linux environment. Supported managed
runtimes are selected from the runtime version in the build plan. Adding
another serving dependency automatically selects the container-image path.
Use `--runner=docker` explicitly to force an image for an otherwise eligible
service.

For image deployments, Anybuild creates the ECR repository when needed, logs
Docker into ECR, and pushes the image. Managed-runtime deployments upload the
`.zip` directly and do not use ECR. Creating either kind of function also
requires `--aws-role` with its IAM execution role ARN; updates do not. An
AWS does not allow switching an existing function between `Zip` and `Image`.
If the selected artifact differs from the function's current package type,
rebuild with `--runner=docker` or recreate the function as appropriate.

Use `--aws-profile`, `--aws-repository`, `--aws-image-tag`, and
`--aws-architecture` to override their defaults. The public Lambda Web Adapter
layer is selected automatically for managed runtimes; use
`--aws-lambda-adapter-layer` to override its ARN. The AWS CLI must be installed
and authenticated. A Docker-compatible client is also required for building
and for image deployments.

The deploy command manages the function artifact, but does not create public
function URLs or other event triggers.

## The Anybuild file

`anybuild generate` writes an editable Starlark file containing the detected,
typed provider configuration alongside the provider build and serve:

```python
# Generated by Anybuild v0.23.0 — https://anybuild.run
# This file is yours to edit: add steps or override the serve below.

load("//anybuild/tools:python.bzl", "python_build", "python_config", "python_serve")

config = python_config(
    schema = 1,
    commands = {
        "start": "python main.py",
    },
    python_main_file = "main.py",
    python_version = "3.13",
    uv_version = "0.8.15",
)

build = python_build(config)

python_serve(config, build, name = "my-app")
```

The generated values make detection explicit and reproducible. The file is
yours: edit the config, add steps around the build (`build_pre` /
`build_post`), extend the runtime (`extra_deps`, `extra_env`), attach services,
or compose builds and serves from different providers (a Hugo build served by
the shared static serve, a PHP build with a Node asset build folded in). Any
config field can also be overridden without editing the file, via
`ANYBUILD_*` environment variables (e.g. `ANYBUILD_PHPIX=true`) or `--config`
JSON. Legacy `SHIPIT_*` variables remain supported when the corresponding
`ANYBUILD_*` variable is absent.

See [docs/anybuild-files.md](docs/anybuild-files.md) for the full format
reference: builtins, `file_exists()`, load labels, the serve override
surface, and composition examples.

## Supported Technologies

Anybuild detects the following frameworks and tools. The configuration column
shows the value written to a generated `Anybuild` file; in most cases, you can
let `anybuild generate` detect it for you.

| Ecosystem | Framework or tool | Configuration | Provider |
| --------- | ----------------- | ------------- | -------- |
| Node.js | Angular | `node_framework = "angular"` | `node`, `node-static` |
| Node.js | Assemble | `node_framework = "assemble"` | `node`, `node-static` |
| Node.js | Astro | `node_framework = "astro"` | `node`, `node-static` |
| Node.js | Brunch | `node_framework = "brunch"` | `node`, `node-static` |
| Node.js | Create React App | `node_framework = "create-react-app"` | `node`, `node-static` |
| Node.js | Docusaurus | `node_framework = "docusaurus"` or `"docusaurus-old"` | `node`, `node-static` |
| Node.js | Eleventy | `node_framework = "eleventy"` | `node`, `node-static` |
| Node.js | Ember | `node_framework = "ember"` | `node`, `node-static` |
| Node.js | Gatsby | `node_framework = "gatsby"` | `node`, `node-static` |
| Node.js | Harp | `node_framework = "harp"` | `node`, `node-static` |
| Node.js | Hexo | `node_framework = "hexo"` | `node`, `node-static` |
| Node.js | Hydrogen | `node_framework = "hydrogen"` | `node` |
| Node.js | Ionic Angular | `node_framework = "ionic-angular"` | `node`, `node-static` |
| Node.js | Ionic React | `node_framework = "ionic-react"` | `node`, `node-static` |
| Node.js | Mastra | `node_framework = "mastra"` | `node` |
| Node.js | Metalsmith | `node_framework = "metalsmith"` | `node`, `node-static` |
| Node.js | NestJS | `node_framework = "nestjs"` | `node` |
| Node.js | Next.js | `node_framework = "next"` | `node`, `node-static` |
| Node.js | Nuxt | `node_framework = "nuxt"` or `"nuxt3"` | `node`, `node-static` |
| Node.js | Parcel | `node_framework = "parcel"` | `node`, `node-static` |
| Node.js | Polymer | `node_framework = "polymer"` | `node`, `node-static` |
| Node.js | Preact | `node_framework = "preact"` | `node`, `node-static` |
| Node.js | React Router | `node_framework = "react-router"` | `node` |
| Node.js | Remix | `node_framework = "remix"`, `"remix-old"`, `"remix-v2"`, or `"remix-v2-classic"` | `node`, `node-static` |
| Node.js | Sanity | `node_framework = "sanity"` or `"sanity-v3"` | `node`, `node-static` |
| Node.js | SolidStart | `node_framework = "solidstart"` | `node` |
| Node.js | Stencil | `node_framework = "stencil"` | `node`, `node-static` |
| Node.js | Storybook | `node_framework = "storybook"` | `node`, `node-static` |
| Node.js | Svelte | `node_framework = "svelte"` | `node`, `node-static` |
| Node.js | SvelteKit | `node_framework = "sveltekit"` | `node`, `node-static` |
| Node.js | TanStack Start | `node_framework = "tanstack-start"` | `node`, `node-static` |
| Node.js | UmiJS | `node_framework = "umijs"` | `node`, `node-static` |
| Node.js | Vite | `node_framework = "vite"` | `node`, `node-static` |
| Node.js | VitePress | `node_framework = "vitepress"` | `node`, `node-static` |
| Node.js | Vue | `node_framework = "vue"` | `node`, `node-static` |
| Node.js | VuePress | `node_framework = "vuepress"` | `node`, `node-static` |
| Node.js | xMCP | `node_framework = "xmcp"` | `node` |
| Node.js server | Elysia | `node_server = "elysia"` | `node` |
| Node.js server | Express | `node_server = "express"` | `node` |
| Node.js server | Fastify | `node_server = "fastify"` | `node` |
| Node.js server | H3 | `node_server = "h3"` | `node` |
| Node.js server | Hono | `node_server = "hono"` | `node` |
| Node.js server | Koa | `node_server = "koa"` | `node` |
| Node.js server | Nitro | `node_server = "nitro"` | `node` |
| Python | Django | `python_framework = "django"` | `python` |
| Python | FastAPI | `python_framework = "fastapi"` | `python` |
| Python | FastHTML | `python_framework = "python-fasthtml"` | `python` |
| Python | Flask | `python_framework = "flask"` | `python` |
| Python | MCP | `python_framework = "mcp"` | `python` |
| Python | Streamlit | `python_framework = "streamlit"` | `python` |
| PHP | Drupal | `php_framework = "drupal"` | `php` |
| PHP | Laravel | `php_framework = "laravel"` | `laravel`, `php` |
| PHP | Moodle | `php_framework = "moodle"` | `php` |
| PHP | Symfony | `php_framework = "symfony"` | `php` |
| CMS | WordPress | Automatically detected | `wordpress` |
| Static site | Hugo | Automatically detected | `hugo` |
| Static site | Jekyll | Automatically detected | `jekyll` |
| Static site | MkDocs | Automatically detected | `mkdocs` |

Anybuild works with three execution environments:

- Local builder for fast, host-native builds.
- Docker builder when container isolation is required.
- Wasmer runner for portable WebAssembly packaging and deployment.

## Rust SDK

The `anybuild` crate exposes the same project pipeline without invoking the
CLI:

```rust
use anybuild::{Anybuild, BuildOptions, RunOptions};

let project = Anybuild::new(".")
    .with_subdir("apps/web")
    .with_env("ANYBUILD_NODE_VERSION", "22");

let plan = project.plan(Default::default())?;
let build = project.build(BuildOptions::default())?;
let run = project.run(RunOptions::default().start())?;

# Ok::<(), anybuild::Error>(())
```

Generation, planning, building, running, deployment, and the combined
`auto` pipeline all return structured outcomes. The SDK does not print its
own diagnostics; callers can install an `EventHandler` with
`with_event_handler`. Child processes inherit the terminal by default, or
can report captured output as events with `ProcessIo::Events`.

## Development

Anybuild is a Rust workspace. Build and run the CLI with:

```bash
cargo run -- . --start
```

Use any subcommand during development by prefixing with `cargo run --`,
for example `cargo run -- build . --wasmer`.

### Tests

Run the gate suites (plan snapshots, config fixtures, generated-file
goldens, unit tests) with:

```bash
cargo nextest run --workspace
```

The end-to-end suite builds and serves the `examples/` projects with the
real binary. Run a wasmer-mode slice with:

```bash
cargo build && cargo nextest run --profile e2e -p anybuild-cli --test e2e \
  --run-ignored all -E 'test(/^node__wasmer__/)'
```

Suites are sliced by test-name prefix (`static`, `staticpython`,
`staticnode1`, `staticnode2`, `python`, `node`, `php`) and build mode
(`__local__`, `__wasmer__`, `__wasmer_and_docker__`). The full gate
stack, including the CLI smoke check, is:

```bash
scripts/verify_rust.sh          # gates
scripts/verify_rust.sh --e2e    # + wasmer-mode e2e
```

Plan snapshots (`tests/plan_snapshots/`) and compatibility fixtures
(`fixtures/`) are committed. The test gates fail if the fixture
manifest is missing or its expected coverage shrinks. For a local checkout
that intentionally omits fixtures, set
`ANYBUILD_ALLOW_MISSING_FIXTURES=1`.

For an intentional plan or config change, regenerate the fixtures from
the current implementation and review the diff like any golden:

```bash
scripts/update_fixtures.sh
```

This rewrites the manifest configs, the generated `Anybuild` texts
(`examples/*/Anybuild` and the manifest's example-derived cases), and the
plan snapshots, then re-runs the gates. Synthetic-case workspaces and
texts, and the `legacy_anybuild` entries (frozen main-era history), are
never regenerated. When adding or removing a case or example, add the
manifest entry / example directory by hand, bump the pinned `EXPECTED_*`
counts in the gate tests, and run the script to fill in the derived
fields.

### Release Automation

Releases are automated with Release Please.

Requirements:

* Enable GitHub Actions workflow permissions for creating pull requests in the
  repository settings. Release Please uses the built-in `GITHUB_TOKEN`.

On each push to `main`, Release Please opens or updates a release PR based on
Conventional Commits. Use `fix:` for a patch, `feat:` for a minor release, and
a `!` or `BREAKING CHANGE` footer for a major release. The release PR updates
the Rust workspace versions, dependency references, lockfile, manifest, and
changelog.

The internal `anybuild-workspace` package coordinates the single workspace
version and is never published. The two public crate manifests use explicit
versions because Release Please requires literal `[package].version` values.

Merging the release PR creates a draft `vX.Y.Z` GitHub release. Native GitHub
runners build macOS Intel and ARM64, static Linux x86-64 and ARM64, and Windows
x86-64 archives. The workflow smoke-tests each binary, generates and verifies
`SHA256SUMS`, uploads all assets, and only then publishes the release. It does
not publish either crate to crates.io.

Normal CI runs that same five-target release build matrix on every pull request
and push to `main`. It smoke-tests each binary and uploads temporary workflow
artifacts, catching target-specific failures before a release is prepared.

If an artifact build fails, the release remains a draft. Rerun the Release
Artifacts workflow manually with that draft tag after fixing the failure.
