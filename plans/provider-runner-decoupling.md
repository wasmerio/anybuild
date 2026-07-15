# Plan: Decouple providers, targets, builders, and runners

## Status

Future proposal for review. No implementation work has started.

## Summary

The Rust port currently lets a runner modify provider configuration before
Starlark evaluation, rewrite build steps, query a live build backend, package
artifacts, execute commands, and expose Wasmer-only deployment through a
downcast.

This plan separates those responsibilities by adding an explicit target
compilation stage. A target builder will keep provider configuration typed
while preparing and lowering a plan. It will serialize provider data only at
the boundaries where Starlark or a runner requires JSON.

The intended pipeline is:

```text
Provider detection
    -> typed ProviderConfig
Target builder preparation
    -> typed evaluation config + RuntimeSpec
Starlark evaluation
    -> portable Serve
Target builder finalization
    -> ExecutionPlan
Build backend
    -> BuildArtifacts
Runner / packager
```

The first tranche removes the direct provider-to-runner dependency and the
target-layout boolean. Later tranches remove the runner's live reference to a
build backend and split deployment from ordinary execution.

## Goals

- Remove the `shipit-run -> shipit-providers` crate dependency.
- Keep all provider mutation typed until a JSON-consuming boundary.
- Preserve plan and annotation output byte-for-byte.
- Make target adaptation a pure, independently testable operation.
- Make the evaluation layout an explicit target property.
- Replace shared mutable backend access with immutable build artifacts.
- Narrow runner interfaces to the capabilities their callers need.
- Preserve local, Docker, Wasmer, and run-without-build behavior.

## Non-goals

- No provider behavior or generated plan changes.
- No changes to the Starlark provider APIs.
- No changes to Wasmer manifest or annotation schemas.
- No plugin system for external providers or runners in this work.
- No general-purpose compilation framework beyond the current targets.
- No requirement to convert every runtime distinction into a public enum.

## Current coupling

### Runners consume provider types

`Runner::prepare_config` accepts and returns `ProviderConfig`. This makes the
runner crate depend on every provider configuration variant.

Current interface:

```rust
pub trait Runner {
    fn prepare_config(
        &mut self,
        config: ProviderConfig,
    ) -> ProviderConfig;
}
```

See
[`crates/shipit-run/src/lib.rs`](../crates/shipit-run/src/lib.rs) and
[`crates/shipit-run/Cargo.toml`](../crates/shipit-run/Cargo.toml).

### Wasmer target policy lives in the runner

The Wasmer runner currently owns three kinds of policy:

- plan-visible Python/WASIX configuration changes;
- annotation-only PHP and Node configuration changes;
- Go dependency and environment lowering for WASIX.

It also derives the Wasmer app kind from provider and framework identity.
These operations decide what should be built for a target. They are target
compilation, not command execution.

See
[`crates/shipit-run/src/wasmer.rs`](../crates/shipit-run/src/wasmer.rs).

### Runner selection affects evaluation

The CLI creates a runner, asks it to mutate the provider config, and only then
evaluates the Shipit file. Evaluation layout also branches on a `wasmer` bool.

See
[`crates/shipit-cli/src/context.rs`](../crates/shipit-cli/src/context.rs).

### Runners retain a live build backend

Local and Wasmer runners retain an `Rc<RefCell<dyn BuildBackend>>`. They query
it for deterministic mount paths and for runtime state discovered during a
build. This combines layout description, mutable build state, and runner
execution in one shared object.

### Deployment requires a concrete downcast

The deploy command obtains a generic runner and downcasts it to
`WasmerRunner`. This indicates that deployment is not a common runner
capability.

See the current
[`deploy` command](../crates/shipit-cli/src/commands/deploy.rs).

## Architectural decisions

### 1. Use a target builder instead of `PreparedProvider`

There will be no public `PreparedProvider` transfer object. The target builder
will own the state that must survive across Starlark evaluation:

- the typed plan-visible provider config;
- the typed annotation config;
- the runtime and layout decisions derived from those configs.

The builder represents a compilation session rather than a collection of
optional setters. Its constructor returns an already valid prepared state.

Conceptual use:

```rust
let target = WasmerTargetBuilder::new(provider_config)?;

let serve = evaluate_shipit(EvaluateOptions {
    config: target.evaluation_config_json()?,
    layout: target.evaluation_layout(artifact_layout),
    // ...
})?;

let execution_plan = target.finish(serve)?;
```

A typestate parameter is not initially needed. There is no useful public
partially initialized state: `new` either returns a valid builder or an error.

### 2. Keep the compiler typed inside

The target builder will depend on `ProviderConfig` and match typed provider
variants where target-specific behavior requires it.

It must not implement provider changes by inserting keys into
`serde_json::Value`. Raw JSON mutation can append changed keys at the end of a
map, which changes byte output even if the values remain semantically equal.

For Wasmer, the builder will maintain two typed configs:

```rust
pub struct WasmerTargetBuilder {
    evaluation_config: ProviderConfig,
    annotation_config: ProviderConfig,
    runtime: RuntimeSpec,
}
```

The configurations have intentionally different semantics:

- `evaluation_config` includes plan-visible WASIX changes;
- `annotation_config` starts from the evaluation config and additionally
  receives runner-only PHP and Node changes.

The builder will also derive `app_kind` from typed framework enums.

### 3. Make JSON opaque only at the runner boundary

`shipit-run` must not receive `ProviderConfig`. The finalized execution plan
will expose only runner-facing data:

```rust
pub struct ExecutionPlan {
    pub serve: Serve,
    pub build_steps: Vec<Step>,
    pub runtime: RuntimeSpec,
    pub provider_name: String,
    pub provider_annotation: Option<serde_json::Value>,
}
```

Provider metadata is flattened into `ExecutionPlan`; a separate
`ProviderMetadata` type is not required initially. It can be introduced later
if multiple consumers need to validate or transport those fields together.

The runner may write the opaque annotation into a manifest. It must not read
provider fields from it, add provider fields to it, or use it to choose
runtime behavior.

### 4. Treat byte parity as a hard contract

Shipit relies on stable field order. Provider structs serialize in declaration
order, and the workspace enables the `preserve_order` feature in
`serde_json`.

Each typed configuration will be serialized once for its consumer:

- `evaluation_config` is serialized for Starlark evaluation;
- `annotation_config` is serialized for the Wasmer annotation.

Recursive null removal for the annotation belongs in target finalization. The
runner should receive the final annotation value and write it unchanged.

The existing
`test_config_annotation_matches_python_byte_for_byte` test remains a hard
gate. `plan --wasmer` output must also remain byte-identical.

### 5. Make serve layout part of `RuntimeSpec`

Target layout is needed before Starlark evaluation because `mount()` and
`volume()` produce plan-visible paths. The target builder will therefore
produce its `RuntimeSpec` during construction, not after evaluation.

Initial shape:

```rust
pub struct RuntimeSpec {
    pub serve_layout: ServeLayout,
    pub app_kind: Option<String>,
    pub runtime_kind: RuntimeKind,
}

pub enum ServeLayout {
    BuildArtifacts,
    Wasmer,
}
```

The exact names are reviewable. The important invariant is that evaluation
consults `RuntimeSpec::serve_layout` rather than an unrelated `wasmer` bool.

The evaluation layout remains composite:

- build and volume paths come from the artifact layout;
- serve paths come from the target's serve layout.

### 6. Separate deterministic layout from discovered build state

Mount and volume paths can be computed before a build. The runtime `PATH` can
be discovered during a local or Docker build. The build contract should make
that distinction visible:

```rust
pub struct ArtifactLayout {
    pub build_root: PathBuf,
    pub artifact_root: PathBuf,
    pub volume_root: PathBuf,
}

pub struct BuildArtifacts {
    pub layout: ArtifactLayout,
    pub runtime_path: Option<String>,
}
```

The backend will support both building and describing an existing build:

```rust
pub trait BuildBackend {
    fn artifacts(&self) -> BuildArtifacts;

    fn build(
        &mut self,
        request: &BuildRequest,
    ) -> Result<BuildArtifacts>;
}
```

`artifacts()` must be pure. It allows evaluation before a build and supports
`shipit run` against artifacts created by an earlier invocation.

The `runtime_path` returned by `artifacts()` may be absent when no current or
persisted build state provides it. Runners must continue to support existing
artifacts that do not require it.

## Target crate and dependency direction

Add a `shipit-targets` crate for target preparation and lowering.

```text
shipit-plan
    neutral plan, runtime, layout, and artifact contracts

shipit-providers
    detection and typed provider configuration

shipit-targets
    depends on shipit-plan and shipit-providers
    typed LocalTargetBuilder and WasmerTargetBuilder

shipit-build
    depends on shipit-plan
    executes build requests and returns BuildArtifacts

shipit-run
    depends on shipit-plan
    packages and executes finalized plans

shipit-cli
    selects and orchestrates all of the above
```

The dependency gate is:

```text
shipit-run must not directly or transitively depend on shipit-providers
```

Neutral contracts consumed by runners must therefore live in `shipit-plan`,
not in `shipit-targets`.

## Target builder responsibilities

### Local target

The local target builder initially performs identity transformations:

- preserve the typed provider config;
- select `ServeLayout::BuildArtifacts`;
- leave build steps unchanged;
- produce no provider annotation unless a local consumer requires one.

Keeping a local builder makes target selection explicit and gives both targets
the same orchestration lifecycle.

### Wasmer target

During construction, `WasmerTargetBuilder` will:

1. Clone or move the detected typed provider config.
2. Apply plan-visible Python/WASIX changes to `evaluation_config`.
3. Clone that result into `annotation_config`.
4. Apply annotation-only PHP and Node changes to `annotation_config`.
5. Derive `RuntimeSpec`, including Wasmer serve layout and app kind.

Before evaluation it exposes:

- ordered JSON serialized from `evaluation_config`;
- an evaluation layout driven by `RuntimeSpec::serve_layout`.

During `finish`, it will:

1. Lower target-specific build steps.
2. Replace Go with Go-WASIX where required.
3. Add the required `GOOS` and `GOARCH` environment step.
4. Normalize and serialize `annotation_config`.
5. Return a complete `ExecutionPlan`.

All of these operations must return errors rather than silently discarding an
invalid transformation.

## Runner responsibilities after tranche one

The runner will no longer prepare providers or rewrite build steps.

An interim interface may be:

```rust
pub trait Runner {
    fn package(
        &mut self,
        plan: &ExecutionPlan,
    ) -> Result<()>;

    fn prepare(
        &mut self,
        env: &IndexMap<String, String>,
        prepare: &[RunStep],
    ) -> Result<()>;

    fn has_serve_command(&self, command: &str) -> bool;

    fn run_serve_command(
        &mut self,
        command: &str,
        volumes: Option<&IndexMap<String, String>>,
        env: Option<&IndexMap<String, String>>,
    ) -> Result<()>;
}
```

This is intentionally interim. Artifact injection and capability splitting
belong to later tranches so the provider boundary can land independently.

## Migration plan

### Tranche 1: target compilation and layout

This tranche removes provider coupling and the layout boolean without changing
the build-backend ownership model.

1. Add neutral `RuntimeSpec`, `ServeLayout`, and `ExecutionPlan` contracts to
   `shipit-plan`.
2. Add `shipit-targets` with local and Wasmer target builders.
3. Move the plan-visible and annotation-only config changes from
   `WasmerRunner::prepare_config` into `WasmerTargetBuilder::new`.
4. Keep both configurations typed and serialize them independently.
5. Move app-kind derivation into the Wasmer target builder.
6. Drive `EnvironmentLayout` from `RuntimeSpec::serve_layout` and remove its
   `wasmer` bool.
7. Move `prepare_build_steps` into `WasmerTargetBuilder::finish` as a pure
   lowering operation.
8. Pass the finalized opaque annotation and app kind to the runner through
   `ExecutionPlan`.
9. Remove `prepare_config` and `prepare_build_steps` from `Runner`.
10. Remove `shipit-providers` from `shipit-run` dependencies.

Temporary compatibility adapters are acceptable within a commit series, but
the merged tranche must satisfy the final dependency gate.

#### Tranche 1 acceptance gates

- All existing plan snapshots pass unchanged.
- `plan --wasmer` output remains byte-identical.
- `test_config_annotation_matches_python_byte_for_byte` passes unchanged.
- Existing Wasmer annotation tests pass unchanged.
- Local and Wasmer mount-path tests cover app and named mounts.
- A regression test proves Wasmer layout is selected without a boolean in the
  evaluation context.
- Go-to-Go-WASIX lowering is covered by pure target-builder tests.
- PHP and Node runner-only changes do not affect the evaluated plan.
- The `shipit-run` manifest and dependency graph contain no provider crate.
- Formatting, warning-denied build, Clippy, and the Rust workspace tests pass.

### Tranche 2: immutable build artifacts

1. Add `ArtifactLayout`, `BuildRequest`, and `BuildArtifacts` to the neutral
   contract layer.
2. Add the pure `BuildBackend::artifacts()` accessor.
3. Change `BuildBackend::build` to return `BuildArtifacts`.
4. Pass `ArtifactLayout` into evaluation instead of querying a mutable backend.
5. Pass post-build `BuildArtifacts` into local and Wasmer packaging.
6. Update `shipit run` to construct runners from existing artifacts.
7. Remove `Rc<RefCell<dyn BuildBackend>>` from runners.

#### Tranche 2 acceptance gates

- Local and Docker builds return equivalent deterministic layouts.
- Docker's discovered runtime `PATH` reaches local serve-script generation.
- `shipit run` works in a fresh process after a previous build.
- Wasmer volume and artifact mapping tests pass without a live backend.
- No runner stores a `BuildBackend`, `Rc`, or `RefCell`.
- Existing build, run, and E2E tests pass unchanged.

### Tranche 3: runner capabilities

1. Separate packaging from command execution where the call sites benefit.
2. Introduce a Wasmer deployment capability or construct the deploy client
   directly in the deploy command.
3. Remove `Runner::as_any` and all concrete runner downcasts.
4. Narrow command tests to fakes for the specific capability being exercised.
5. Re-evaluate whether a common `Runner` trait is still useful with only local
   and Wasmer implementations.

#### Tranche 3 acceptance gates

- Deploy contains no runner downcast.
- Test fakes do not implement unrelated packaging or deployment methods.
- Build and run command behavior remains unchanged.
- The full Rust and E2E gates pass.

## Testing strategy

### Pure target tests

Target-builder unit tests should cover:

- Python plan-visible WASIX fields;
- PHP annotation-only `phpix` behavior;
- Node annotation-only EdgeJS and native-binary behavior;
- explicit Node precompile overrides;
- composite providers such as MkDocs, WordPress, and Laravel;
- typed app-kind derivation;
- Go dependency lowering and environment insertion;
- local identity behavior.

These tests should assert both the typed intermediate state and serialized
boundary output where order matters.

### Integration tests

CLI and integration tests should cover the full ordering:

```text
detect -> target builder -> evaluate -> finish -> build -> package
```

Important cases include:

- local versus Wasmer plan parity outside intentional target differences;
- app and named mount paths for local, Docker, and Wasmer combinations;
- annotation preservation through manifest generation;
- running an existing local build without rebuilding;
- running an existing Wasmer package without provider detection.

### Dependency tests

The crate manifests are the primary architecture enforcement. CI should fail
if `shipit-run` adds `shipit-providers` directly or through a target-contract
crate. A lightweight dependency check may supplement manifest review.

## Risks and mitigations

### Ordered JSON changes

Risk: a target transformation mutates raw JSON and changes field order.

Mitigation: typed mutation only, one serialization per consumer, and the
existing byte-parity test as a mandatory gate.

### Evaluation and annotation configs accidentally converge

Risk: runner-only PHP or Node fields leak into Starlark evaluation.

Mitigation: retain two typed configs in the builder and test their differences
before serialization.

### Layout is computed too late

Risk: `RuntimeSpec` is created only after evaluation, restoring a target
boolean or hardcoded layout in the evaluator.

Mitigation: require target-builder construction to produce `RuntimeSpec`
before `evaluate_shipit` is called.

### ExecutionPlan becomes a generic metadata bag

Risk: arbitrary provider configuration gradually leaks back into runner logic.

Mitigation: keep the opaque annotation write-only in runners and add explicit
runtime fields for every behavior the runner must select.

### Run-without-build regresses

Risk: making `build()` return artifacts assumes every runner invocation builds
first.

Mitigation: require a pure `artifacts()` path and test fresh-process run flows.

### The migration changes too many seams at once

Risk: target, backend, and capability changes make parity failures difficult
to isolate.

Mitigation: land the three tranches independently with their own hard gates.

## Review questions

1. Is a dedicated `shipit-targets` crate preferable to a target module in the
   CLI crate?
2. Are flattened `provider_name` and `provider_annotation` fields sufficient,
   or is a small `ProviderMetadata` value useful now?
3. Should `RuntimeSpec` contain only pre-evaluation decisions, or should it
   also own finalized packaging hints such as app kind?
4. Are `ServeLayout::BuildArtifacts` and `ServeLayout::Wasmer` the right
   abstraction names, or should layout be represented by concrete roots?
5. Does runtime `PATH` need persistence for any run-without-build scenario, or
   is it sufficient for previously generated local scripts to retain it?
6. After capability splitting, is dynamic runner dispatch still valuable, or
   would an enum make the supported target set clearer?

## Completion criteria

This plan is complete when:

- target-specific provider behavior lives outside runners;
- target layout is explicit before evaluation;
- runners consume only neutral plans and opaque annotations;
- runners do not retain live build backends;
- deployment requires no concrete downcast;
- all parity, Rust, and E2E gates remain green.
