# Migrating shipit to Rust

## Why this is tractable now

The Starlark provider refactor was, in hindsight, the enabling move for
this migration. Three facts fall out of it:

1. **The plan-construction logic is no longer Python.** All build/serve
   assembly lives in `src/shipit/starlib/*.shipit` (~1,200 lines of
   Starlark). It carries over to Rust *unchanged* — it is the spec, not
   code to port.
2. **The Starlark engine is already starlark-rust.** Python talks to it
   through the `xingque` bindings; Rust talks to it natively. Same
   dialect, same semantics, one less layer.
3. **The verification fixtures are language-neutral.** The 98 plan
   snapshots (`tests/plan_snapshots/`) are path-tokenized JSON ("app",
   "opt/venv", `<WORKSPACE>`), the 80 generated `Shipit` goldens are
   version-masked text, and the e2e suite invokes `shipit` as a
   subprocess. All three can gate a Rust binary without modification.

What remains in Python is ~7,400 lines with clear seams: detection +
config (providers), the evaluation host (builtins), the CLI pipeline,
two build backends, and two runners.

## Goals

- Single static binary; no `uv`/Python needed to run shipit itself
  (builds of Python projects still use `uv` — inside the plan, as today).
- 100% behavioral parity: every current use case, gated by the existing
  fixtures rather than by re-review.
- Clean architecture: the crate boundaries mirror the seams the Python
  refactor already carved (types / eval host / providers / build / run /
  cli), with the dependency arrows enforced by the compiler.
- Keep `uvx shipit-cli` working (wheels that ship the prebuilt binary).

## Non-goals

- No new features during the port. The Python implementation is frozen
  except for bug fixes; any intentional plan change lands in Python
  first and regenerates the shared fixtures, so Rust always tracks a
  single source of truth.
- No Starlark stdlib changes (beyond the two already-deferred fixes,
  which are Starlark-side and implementation-independent).
- Legacy fully-inlined Shipit files keep evaluating (same bar as today:
  all 80 main-era generated files pass; hand-edit corners documented).

## Target architecture

Cargo workspace at the repo root, one crate per seam:

```
crates/
  shipit-plan/        Step/Serve/Mount/Volume/Service/Package types (serde)
  shipit-starlark/    module loader, builtins (Ctx), config bridge
  shipit-providers/   detection + config + install-context discovery
  shipit-build/       step executors: local, docker
  shipit-run/         runners: local, wasmer
  shipit-cli/         clap commands, the resolve_project_context pipeline, UI
src/shipit/starlib/   (shared) embedded into the binary via include_dir!
assets/               (shared) php.ini, wordpress/*, optimize-node-modules.sh
```

Dependency direction: `cli → {providers, starlark, build, run} → plan`.
Nothing depends on `cli`; `starlark` knows nothing about providers (it
receives a config *value*, mirroring today's `config_view`).

### shipit-plan

Direct port of `shipit_types.py`. One deliberate choice: every
plan-visible map (env vars, commands) is an `IndexMap`, because Python
dicts preserve insertion order and plan JSON is the contract.

### shipit-starlark

The evaluation host, mirroring `evaluator.py` + `starlark_loader.py`:

- Same `Dialect` flags (def, lambda, load, keyword-only args, top-level
  statements, f-strings) and library extensions (StructType, Json,
  Print, Pprint, Partial, Map, Filter, Debug, RecordType as needed).
- Module graph loader: labels resolve as today (`//shipit/...` →
  embedded stdlib, `//pkg:file` → project root, relative → loading
  file); cache and cycle detection keyed by **resolved path**; static
  typechecking on.
- Builtins registered on a `GlobalsBuilder`: `dep run copy workdir env
  use write path mount volume service file_exists serve` + the `_serve`
  alias (the stdlib `serve()` shadows the builtin) + `PORT` + `config`.
- Plan objects (`CtxMount`, steps, packages) are `StarlarkValue` impls
  with attribute access — the stdlib reads `.path`, `.serve_path`,
  `.steps`, `getattr(step, "group", None)`, and passes them around
  opaquely, exactly as the Python host does.
- The config bridge is the Rust `config_view`: a read-only Starlark
  value over the config — enums as strings, sets as sorted lists,
  nested structs attribute-accessible. Honest attribute access (unknown
  fields raise), same as today.

### shipit-providers

Port of `providers/*.py`. The recent decomposition into pure module
functions (`_detect_framework`, `_detect_server`, `_compute_install_
inputs`, ...) makes this mostly mechanical; those functions and their
unit tests translate one-to-one. The config layer:

- serde structs, one per provider, with the base fields (name, port,
  commands, app_subdir).
- The `SHIPIT_*` env overlay is hand-rolled (not a framework) so we can
  match pydantic-settings semantics exactly — including its bool
  parsing set ("true"/"1"/"yes"/"on", case-insensitive) — and pin it
  with ported tests.
- `--config` JSON merges over detected values, as today.
- `install_context.py` (uv workspaces, requirements includes, package
  manifests) ports against the `toml` + `serde_json` + `globset`
  crates.

### shipit-build / shipit-run

- Local backend: step executor over `std::process` + fs ops; copy-with-
  ignore via the `ignore`/`globset` crates; mount layout identical
  (`build/app`, `build/opt/<name>`).
- Docker backend: same synthesized-execution approach, subprocess to
  the docker client (client selectable, e.g. depot).
- Local runner: prepare + start, `$PORT` env.
- Wasmer runner is the largest single port (~830 lines): wasmer.toml
  generation (via `toml_edit`, the Rust tomlkit — preserves user
  edits), app.yaml handling (`serde_yaml`), the `anybuild.run/*`
  annotations, capabilities/services, deploy via the `wasmer` CLI
  subprocess. Two invariants to carry over verbatim:
  - `prepare_config` split: the python trio (cross_platform, extra
    index, precompile) mutates the plan-visible config *before* eval;
    the php/node flips (phpix, edgejs, remove_native_binaries) apply
    only to a runner-side copy used for metadata. Sharing the mutation
    breaks php routing — this is pinned by `test_wasmer_annotations`.
  - `resolve_app_kind` keys off `serve.provider` (serve-side identity).

### shipit-cli

clap-based, same command surface (`auto`, `generate`, `plan`, `build`,
`run`, `deploy`) and flags. The shared pipeline is a direct port of
`resolve_project_context` (paths → subdir marker → backend/runner →
config → prepare_config → evaluate, with `project_root` threaded).
Output strings that the e2e suite pattern-matches ("Build complete ✅",
serve banners) are preserved verbatim — they are part of the contract.

## Semantics that must survive (the hard-won list)

Each of these was a real bug or a deliberate decision this year; the
Rust port inherits them as tests, not as lore:

- `file_exists()` is app-dir-scoped with a *lexical* escape check
  (rejects `..`, never resolves symlinks — symlinked `wp-content` works).
- Module cache keyed by resolved path, not label string.
- Generated files bake `name = "<dir>"`; the provider label is
  serve-side; `app_subdir = "..."` marker is read back by the CLI.
- Command overrides: replace-first-of-group for build/install (later
  same-group steps survive), start/after_deploy override, `$PORT`
  substitution. Applied in Starlark for stdlib files; the CLI-side
  replay exists only for legacy no-`load()` files — in Rust, gate it on
  "entry has no load statements" from day one (this fixes deferred
  review finding 4 as part of the port instead of replicating the
  unconditional replay).
- Cross-platform wheel steps are gated on an install branch existing
  (pyproject/requirements/extra deps) — bare-Procfile apps get none.
- `serve()` None-filters build/deps/services/mounts/volumes;
  conditional-`None` steps are idiomatic everywhere.
- Env PORT coerced to int before entering config; plan JSON port is a
  number.
- Insertion order of env-var and command maps is plan-visible.

## The equivalence harness (how we know it works)

Ordered strongest-first; all reuse existing fixtures:

1. **Plan snapshots (98)**: a Rust test evaluates every example (plus
   the `__cross` cross-platform dimension and the synthetic subdir
   cases) and compares against `tests/plan_snapshots/*.json`
   byte-for-byte, using the same normalization (null-key dropping,
   mount-path stripping, `<WORKSPACE>` tokens, sorted keys).
2. **Generated-file goldens (80)**: `shipit generate` output must match
   `examples/*/Shipit` under the same version mask.
3. **Config differential**: for every example, dump the detected
   provider config as JSON from both implementations and diff — this
   isolates detection/config parity from evaluation parity.
4. **e2e reuse**: the pytest e2e suite already spawns `shipit` as a
   subprocess; add a `SHIPIT_BIN` env indirection so the same suite
   (local mode and Wasmer/CI mode, all technology slices) drives either
   binary. The suites stay in Python indefinitely — they are test
   harnesses, not product code.
5. **Differential CI job** during the transition: run both binaries
   over all examples on every PR and diff normalized plans, so drift is
   caught at the commit that introduces it.

Unit tests for ported pure functions (detection helpers, redirects
parsing, procfile, override semantics, loader/evaluator regression
tests from the review) are translated into the owning crates as each
phase lands.

## Phases

Each phase has a hard gate; nothing merges to the next phase until the
gate is green in CI.

**Phase 0 — Scaffolding (S).** Workspace, `shipit-plan`, snapshot
normalizer/tokenizer, fixture readers, differential-CI skeleton.
Gate: Rust re-serializes the existing snapshots losslessly.

**Phase 1 — Starlark host (M).** Loader, builtins, config bridge,
`evaluate_shipit`. Configs are *injected from JSON* (dumped by the
Python side) rather than detected, which decouples this phase from
providers entirely.
Gate: all 98 snapshots byte-identical with injected configs; the
loader/evaluator regression tests pass; all 80 legacy main-era inlined
files still evaluate.

**Phase 2 — Providers (L).** Detection, config + SHIPIT_ env overlay +
--config merge, install-context discovery, procfile, redirects.
Gate: config differential clean over all examples; ported unit tests
green; snapshots now pass end-to-end (detect → config → evaluate).

**Phase 3 — Generator + read-only CLI (S).** `generate` and `plan`
commands, subdir pipeline, name baking.
Gate: 80 goldens byte-identical (version-masked); `plan` JSON matches
Python's for all examples.

**Phase 4 — Local build + run (M).** Step executor, mounts/volumes,
`build`, `run`, `auto` without docker/wasmer.
Gate: local-mode e2e suite green via `SHIPIT_BIN`.

**Phase 5 — Wasmer + Docker (L).** WasmerRunner port (toml/yaml/
annotations/deploy), docker backend.
Gate: Wasmer-mode e2e green (the CI configuration), annotation tests
ported and green, `deploy --wasmer-deploy-config` output diffed.

**Phase 6 — Cutover (S).** Distribution: `cargo-dist` for binaries +
maturin-built wheels so `uvx shipit-cli` transparently becomes the Rust
binary; `starlib/` and `assets/` move to the repo root as the single
shared source. One release ships both implementations behind a
`SHIPIT_IMPL=python` escape hatch; the Python tree is removed after a
quiet release.

Rough shape: phases 1–3 are the low-risk majority of the value; phases
4–5 carry the operational risk (process management, docker/wasmer CLI
interplay, Windows process groups) and deserve the most e2e soak time.

## Library mapping

| Python                  | Rust                                        |
| ----------------------- | ------------------------------------------- |
| xingque (starlark-rust) | `starlark` (native)                         |
| typer                   | `clap` (derive)                             |
| pydantic + settings     | `serde` + hand-rolled `SHIPIT_*` overlay    |
| rich                    | `console`/`indicatif` (+ preserved strings) |
| tomlkit                 | `toml_edit` (comment/format preserving)     |
| toml                    | `toml`                                      |
| pyyaml                  | `serde_yaml`                                |
| requests                | `ureq` (blocking, small)                    |
| sh / subprocess         | `std::process` (+ `duct` if composition helps) |
| ripgrep-python          | `grep-searcher`/`regex`                     |
| semantic-version        | `semver`                                    |
| dotenv                  | `dotenvy`                                   |

## Risks and mitigations

- **starlark-rust API drift vs the xingque wrapper.** xingque pins the
  same engine, but the native API differs (heaps, frozen modules,
  `StarlarkValue` traits). Mitigation: phase 1 is exactly this risk,
  isolated, with the strongest gate (snapshots + legacy files).
- **pydantic coercion corners.** Bool env parsing, set-vs-list dumps,
  enum values. Mitigation: config differential over all examples plus
  targeted tests for each coercion we actually rely on.
- **tomlkit vs toml_edit formatting.** wasmer.toml diffs may be
  whitespace-different. Mitigation: compare parsed values in tests, and
  keep a formatting golden for the common generation path.
- **Output-string coupling.** e2e matches console phrases. Mitigation:
  treat user-visible strings as fixtures; port them verbatim first.
- **Windows.** Process groups, path separators in plans (snapshots are
  POSIX-tokenized). Mitigation: CI matrix from phase 4 onward.
- **Two implementations drifting during the port.** Mitigation: Python
  freeze + differential CI + fixtures regenerated only from Python.

## What this buys beyond parity

- Startup: `uvx` bootstrap + interpreter start disappears; `shipit
  plan` becomes milliseconds.
- Distribution: one binary for CI images and the Wasmer backend (no
  Python in the loop), while `uvx shipit-cli` keeps working for
  existing users.
- The evaluator, loader, and value bridge lose a whole FFI layer, and
  the crate graph makes the architecture rules (providers can't touch
  runners, the eval host can't see detection) compiler-enforced.
