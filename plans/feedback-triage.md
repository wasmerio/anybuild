# Anybuild feedback: checked findings and proposed fixes

Reviewed on September 12, 2026 against Anybuild **v0.28.4**, commit
`fd81bd8ed779b79b4ca639c3f080168ade6a503b`.

The [feedback ledger][ledger] contains **nine directly assigned Anybuild
issues**. Eight remain actionable; F-015's generation-drift concern is
already covered by passing tests, leaving a cosmetic documentation issue.
Related platform issues appear separately below. This document proposes
changes for review; it does not implement them.

## Recommended priority

“Blocker” below means a release priority, including silent runtime breakage
and unintended inclusion of private files. The original severity is retained
so changes in prioritization are explicit. Size estimates are relative:
small is a contained change; medium spans components or runtime tests; large
needs a packaging design and compatibility matrix.

| Order | ID | Original severity | Recommended priority | Status |
| --- | --- | --- | --- | --- |
| 1 | F-021 | workaround | Blocker: private file exposure | Reproduced |
| 2 | F-005 | blocker | Blocker: MCP examples fail | Confirmed |
| 3 | F-018 | blocker | Blocker: WASIX resolution | Reproduced mechanism |
| 4 | F-025 | workaround | Blocker: missing deps | Reproduced mechanism |
| 5 | F-019 | blocker | Blocker: CLI compatibility | Confirmed in code |
| 6 | F-009 | workaround | High: MCP network binding | Confirmed in code |
| 7 | F-024 | paper-cut | Paper-cut: command discovery | Reproduced |
| 8 | F-003 | paper-cut | Paper-cut: deployment docs | Partly valid |
| 9 | F-015 | paper-cut | Cosmetic: old headers | Drift checks pass |

There are no directly assigned Anybuild entries with severity “wish”.
Optional capabilities extracted from the proposed fixes are listed later.

## 1. F-021: Python copies ignored private files

**Finding.** The default Python staging step copies the source with only
`.venv`, `.git`, and `__pycache__` exclusions. The local backend separately
excludes `.anybuild`; the ledger's exclusion list was not quite exhaustive.
I executed the provider's actual staging steps in a synthetic project,
omitting toolchain installation. Both a gitignored `.env` containing a dummy
value and `private/scratch.txt` appeared in the output. This is more serious
than artifact bloat. No real credentials were used or exposed in this check.

**Implementation.** Change all relevant source-copy branches in
[python.bzl][python-steps], including subdirectory staging, installs requiring
the full source, and final app copying. Reuse the existing
[local Git-ignore walker][local-ignore]. A change only to the final copy
would miss the other paths.

**Critical scope correction.** [Docker copy generation][docker-copy] uses
`step.ignore` but never consumes `step.gitignore`. Adding `gitignore = True`
alone therefore does not fix Docker builds. Implement equivalent source
selection there, preferably using the same walker to stage filtered copy
inputs. Preserve per-step opt-outs and explicitly requested build inputs;
do not blindly translate Git patterns into Docker patterns. Decide how a
user's `.dockerignore` composes with this selection.

**Acceptance.** Root and subdirectory Python builds exclude ignored dummy
secrets and scratch files with both builders. Cover nested rules, negation,
full-source installs, and repeat builds after a file becomes ignored: an
already copied file must not survive in a reused artifact. Retain required
build outputs. Document an explicit opt-out for projects intentionally
building ignored inputs. **Size: medium.**

## 2. F-005: both Python MCP examples use a removed API

**Finding.** [python-mcp][mcp-main] and [python-mcp-chatgpt][chatgpt-main]
import `FastMCP`. Their dependency declarations allow MCP 2.x: the first
uses `mcp>=1.13.1` in **pyproject.toml**, not requirements.txt; the second
uses bare `mcp` in requirements.txt. The [published][mcp-releases] 2.1.1 and
current 2.2.0 wheels both contain a compatibility module that immediately raises
`ModuleNotFoundError` for that import. The upstream [migration guide][mcp-v2]
confirms the replacement and transport API changes.

The first example's committed uv.lock still pins MCP 1.13.1. An installation
that keeps that lock can avoid the failure; this is a confirmed incompatibility
with the allowed 2.x versions, not proof that every checkout fails identically.

**Immediate fix.** Bound these v1 examples to `mcp<2`, retaining the existing
lower bound where present. Regenerate applicable lockfiles and verify the
provider-added `mcp[cli]` dependency respects that bound. Apply the same fix
to the published starter's source after locating its owning repository;
editing this repository alone does not establish that the remote template
has changed.

**Durable fix.** Migrate both examples to `MCPServer` and an explicit supported
major range, together with F-009. Transport parameters belong on `run()` in
v2. Review the ChatGPT example's SSE transport and move to Streamable HTTP
as part of the example's client compatibility check, rather than making
only an import substitution.

**Acceptance.** A clean install imports and starts each example, then an MCP
client initializes and calls its tools. Test a nondefault port and the
supported local and Wasmer paths. The current E2E case list has a Node xMCP
case but no Python MCP cases; generated-file tests cannot catch this failure.
**Size: small containment; medium migration with F-009.**

## 3. F-018: cross compilation freezes versions before target resolution

**Finding.** The [cross-wheel steps][cross-wheels] already supply the WASIX
index to `uv pip compile`. They do not simply export the host lockfile.
Instead, they perform another universal, direct-dependency-only resolution
and pin those results before asking pip to resolve the WASIX dependency
graph. This can select a parent version whose required native wheel does
not exist for the target. Adding an index URL is not a fix; it is present.

Using the project's pinned **uv 0.8.15**, an offline wheel fixture reproduced
the mechanism: compilation selected parent 2.0; cross-install failed because
only core 1.0 had a WASIX wheel. Supplying the original allowed parent range
to the target-aware installer successfully selected parent 1.0/core 1.0.
The [live WASIX index][wasix-core] still listed pydantic-core 2.46.4 variants,
with no 2.46.5, when checked. I did not repeat the original full MCP deploy.

**Recommended design.** Keep host dependency installation separate from
target resolution. Extract target root requirements without inventing exact
pins; preserve their ranges, extras, markers, explicit pins, recursive
requirements, constraints, and provider-added dependencies. Resolve the
whole graph against WASIX wheel availability in one operation. The existing
pip `--platform wasix_wasm32 --only-binary=:all:` path is a practical starting
point; uv 0.8.15's platform choices do not include WASIX.

Persist the resulting target requirements or resolution report alongside
the build. Keep the host lock for host installation, and make the distinct
target resolution visible. Explicit user pins or a target lock must either
be honored or fail with a clear explanation; do not silently relax them.
Document this policy before introducing a new target-lock format. Preserve
the intended index trust policy instead of enabling unrestricted index
mixing as a workaround.

**Acceptance.** Offline fixtures cover backtracking, an impossible exact
pin, native wheels, pure Python wheels, extras, and both manifest formats.
Failures identify the requested package chain, target Python/platform, and
missing wheel. A suggested version must come from a verified compatible
resolution, not a hard-coded pydantic bound. Follow with a real WASIX import
smoke test. **Size: large.**

## 4. F-025: extras disappear from cross requirements

**Finding.** This shares code with F-018 but is independently fixable.
`--no-deps` prevents expansion of transitive requirements, while uv's default
extras stripping removes the remaining indication that an extra was
requested. The [uv documentation][uv-extras] describes this default.

With uv 0.8.15, the offline fixture compiled `review-db[binary]>=1` to bare
`review-db==1.0`; the subsequent WASIX install succeeded without the binary
dependency. Adding **`--no-strip-extras`** preserved `[binary]` and installed
both packages successfully.

**Immediate fix.** Add `--no-strip-extras` to both compilation branches in
[python.bzl][cross-wheels]. Keep this as a focused fix while developing
F-018. Simply removing `--no-deps` would resolve more of the graph without
making that resolution WASIX-aware, potentially worsening native wheel
selection. A direct target resolver should preserve extras by construction.

**Acceptance.** Cover `[binary]`, multiple extras, optional dependencies with
markers, and both pyproject.toml and requirements.txt. Assert the installed
dependency closure, not just the generated command text. Separately test
imports in WASIX. This fixes missing packages; it does **not** establish that
the psycopg native wheel's runtime trap in F-026 is fixed.
**Size: small immediate fix; shared durable work with F-018.**

## 5. F-019: incompatible Wasmer CLI fails after building

**Finding.** The [runner][wasmer-version] already reads `wasmer --version`,
but only to annotate artifacts; there is no compatibility comparison.
[Runtime invocation][wasmer-run] still uses `--volume` where mounts are
needed, and v0.28.4 also uses `--env-file` for run/deploy. The locally installed
Wasmer 7.4.1 exposes these options. The ledger's actual 6.1.0 execution was
not repeated, and the first compatible release has not been established.

**Implementation.** Add a shared Wasmer CLI validation layer used by SDK
build, run, and deploy operations, before expensive work. Check the selected
`--wasmer-bin`, including custom paths. Define a tested minimum version and
check required capabilities; validate development-version handling. Use
7.4.0/7.4.1 as initial test candidates, not an unsupported assertion that
every 7.x release works. Pure generation, planning, and local-only builds
should not require Wasmer.

Reuse the version query and report the binary path, detected version,
required version/capability, and upgrade link. Command failures should name
the generated invocation, with tokens and environment values redacted.
`--skip-prepare` is not a general compatibility fix: later run or deploy
operations can still fail.

**Acceptance.** Fake CLI tests cover old, supported, missing, malformed, and
custom-path binaries and prove rejection happens before building. Run a
small package with the selected minimum and current supported CLI. Registry
upload compatibility remains a separate F-020 integration check.
**Size: medium.**

## 6. F-009: MCP 2.x does not consume injected FastMCP settings

**Finding.** [python_env()][python-env] still sets only `FASTMCP_HOST` and
`FASTMCP_PORT` for MCP. Neither inspected MCP 2.x wheel references these
variables; its HTTP entrypoints default to loopback and port 8000. Anybuild's
Starlark `PORT` constant and command-string substitution are not equivalent
to an application environment variable.

**Implementation.** Define an explicit MCP contract: expose `HOST=0.0.0.0`
and `PORT=<configured port>`, retain v1 aliases during compatibility support,
and have self-running examples read and pass these settings. Merely adding
environment variables cannot configure a v2 server that never reads them.
Use stateless HTTP deliberately for the simple Edge examples; do not force
stateless behavior on arbitrary user applications.

Also check the [inferred MCP CLI command][mcp-command] for non-self-running
apps, which currently does not pass host or port. Choose arguments using a
verified supported SDK/CLI contract. If major-version detection is needed,
use resolved package metadata, not guesses from a dependency range. Include
the cross-installed CLI entrypoint in this check.

**Acceptance.** V1 compatibility and v2 self-running/CLI-managed cases listen
on the configured nondefault port and complete MCP initialization through
an external connection. Validate legitimate public Host headers and keep
appropriate transport security. **Size: medium, grouped with F-005.**

## 7. F-024: root help hides the commands

**Finding.** Rebuilt v0.28.4 reproduces both symptoms. `--help` prints auto
help with no command list; `help` is interpreted as a nonexistent directory.
`buid` behaves the same way. [Argument normalization][cli-main] inserts
`auto` before Clap can recognize root help or diagnose unknown commands.

**Implementation.** Recognize root `-h`, `--help`, and `help [command]`
before inserting `auto`. Preserve implicit auto for no arguments, auto
options, existing paths, and explicit path-shaped arguments. Send an unknown
bare word to Clap's command diagnostics. Preserve escape routes such as
`./build` and `auto <path>` for directories named like commands.

**Acceptance.** Binary-level tests cover root and per-command help, a typo
with a suggestion, `.`, relative/absolute paths, missing explicit paths,
option-first auto, and command-named directories. Parser-only tests miss
the preprocessing responsible for this bug. **Size: small.**

## 8. F-003: explain the deployment paths

**Finding.** README has build/deploy commands, but does not explain their
relationship to the Wasmer CLI and dashboard Git integration. The
[announcement][blog] also omits that relationship. However, the suggestion
to create `examples/python-mcp/` is stale: that directory and the ChatGPT
variant already exist. Fix and link them using F-005/F-009.

**Implementation.** Add a short README guide, linked from the announcement:

- `Anybuild` defines the build and runtime; source `app.yaml` carries Edge
  deployment configuration.
- Anybuild builds the Wasmer package, merges source app configuration, then
  [delegates publishing to the Wasmer CLI][wasmer-deploy].
- `anybuild build --runner=wasmer` followed by
  `anybuild deploy --platform=wasmer` splits the same workflow explicitly.
- A dashboard Git connection and a self-managed Actions deployment can both
  react to a push. Choose one production-branch trigger; link the verified
  instructions for disconnecting the other.

If documenting a direct `wasmer deploy` equivalent, include v0.28.4's
generated environment file as well as the artifact directory. Coordinate
the Git comparison with F-040 in the docs repository.

**Acceptance.** A reader can select one deployment path, locate the two
configuration files, and find a working MCP example. Check documentation
commands against the adapter. **Size: small; shared docs ownership.**

## 9. F-015: v0.23.0 headers are not proof of stale examples

**Finding.** Both MCP headers still show v0.23.0, but `generate --check`
passes for each. More decisively, the existing
[generated_files_match_examples test][goldens] passed for **all 83 examples**.
It compares freshly generated text while masking the version header. The
gate already runs in CI. Even fixture-update mode intentionally skips files
whose only difference is that header.

**Disposition.** Mark the “missing drift check” portion already addressed.
Keep a small documentation task to explain that the header records original
generation, or choose a version-neutral header and update snapshots once.
Do not automatically rewrite user-editable examples in every release solely
to change a version comment. Keep the meaningful generation check, and add
runtime coverage through F-005. **Size: small/optional.**

## Related issues and ownership boundaries

These are dependencies or useful documentation follow-ups, not additional
direct Anybuild defects established by this review.

- **F-017, F-020, F-039: installer, CLI/registry, setup action.** Align the
  supported CLI baseline and upgrade instructions with F-019. Registry
  errors and action outputs need upstream fixes.
- **F-026: WASIX wheel/runtime.** Track the psycopg trap separately; require
  a real DB connection smoke test before declaring that driver supported.
  Extras preservation alone cannot fix it.
- **F-035: Edge jobs, with Anybuild integration.** Recheck using v0.28.4
  before claiming the missing-env symptom persists; see below.
- **F-040: Edge Git documentation.** Share the deployment comparison from
  F-003 and verify the actual disconnect flow.
- **F-041: Wasmer deployment readiness probe.** Upstream owns probe path and
  wording; Anybuild can expose a supported option after that contract exists.

**F-035 requires a version-aware retest.** Current Anybuild writes serve
environment variables, including Python's `PYTHONPATH`, into a dotenv file
and supplies it during deployment. That is different from the v0.28.3
configuration described by the ledger. Whether arbitrary Edge `execute`
jobs now inherit those variables needs a live platform test. Their working
directory also remains a platform question. Anybuild already supports
`extra_commands`, and generated named commands receive the serve cwd; try a
named command for the job before designing a new job abstraction. Do not
mark this fixed without testing both imports and working directory on Edge.

F-004 and F-038 mention Anybuild only as environment context; their missing
sandbox backend/emulator capabilities belong to the SDK/platform. F-013's
Postgres TLS guidance is also upstream; an Anybuild Python/Postgres example
could link the corrected guidance. The remaining ledger entries concern
other products and do not establish an Anybuild fix.

## Proposed change sequence

1. **File selection:** F-021, covering all Python copy paths and Docker
   parity. Land urgently because ignored local files can become artifacts.
2. **Small containment fixes:** F-005 dependency bounds and F-025 extras
   preservation. These can land independently of the larger redesigns.
3. **Wasmer preflight:** F-019, followed by a documented tested CLI baseline.
4. **Target dependency resolution:** F-018, preserving the F-025 behavior and
   adding target-resolution artifacts and failure diagnostics.
5. **MCP v2 support:** F-005/F-009 migration and runtime tests, followed by
   the published starter update. Coordinate its WASIX tests with step 4.
6. **CLI and docs:** F-024 and F-003; close or clarify F-015. These small
   changes can proceed independently of packaging work.

Optional wishes after correctness: `.anybuildignore` or a typed source-copy
policy, artifact file/size summaries, a discoverable WASIX wheel catalogue,
and explicit target-lock lifecycle commands. Each deserves a scoped design;
none is required to begin the immediate fixes above.

## Verification performed and limits

- Rebuilt the current CLI with `cargo build -p anybuild-cli --offline`.
- Reproduced root help and unknown-word behavior with that binary.
- Checked generation for both MCP examples and ran the existing golden test:
  all 83 examples matched after version-header normalization.
- Executed Python provider staging in a synthetic local project and found
  both gitignored dummy files in the resulting app artifact.
- Downloaded and inspected published MCP 2.1.1/2.2.0 wheels, upstream
  migration documentation, and current WASIX wheel-index metadata.
- Reproduced resolution and extras loss with uv 0.8.15 and synthetic local
  wheels; confirmed target backtracking and `--no-strip-extras` remedies.
  These checks used pip 21.2.4 for offline target installation, not the
  dynamically selected `uvx pip` version from a full Anybuild build.
- Checked required flags on installed Wasmer 7.4.1 and inspected runner,
  deployer, Python provider, copy backends, and CI coverage.
- Ran `scripts/verify_rust.sh`: formatting, build, Clippy, and all 319 tests
  that ran passed; 163 E2E cases were ignored. The script then failed at the
  external SDK-consumer check because its lockfile pins Anybuild 0.28.2
  while this workspace is 0.28.4. The final CLI smoke stage was not reached.
  This pre-existing verification issue is separate from the ledger: refresh
  the consumer lockfile and ensure release version bumps keep it current.

No production deployment, remote starter modification, full Python example
runtime test, or live Postgres/job test was performed. Docker's missing
Git-ignore handling and the absent Wasmer version gate were confirmed by
source inspection, not by rerunning the historical environment. Findings
above distinguish those checks from executed reproductions.

Temporary reproduction inputs and logs are under
`/tmp/anybuild-review-evidence/`; they are not committed project tests.

[ledger]: https://github.com/francisco-perez-sorrosal/wasmer-sandbox-mcp/blob/3d8a7df2b832878c0fd40a1244bb1b5933e546da/FEEDBACK.md
[python-steps]: https://github.com/wasmerio/anybuild/blob/fd81bd8ed779b79b4ca639c3f080168ade6a503b/crates/anybuild/resources/starlib/tools/python.bzl#L29-L148
[local-ignore]: https://github.com/wasmerio/anybuild/blob/fd81bd8ed779b79b4ca639c3f080168ade6a503b/crates/anybuild/src/build/local.rs#L95-L136
[docker-copy]: https://github.com/wasmerio/anybuild/blob/fd81bd8ed779b79b4ca639c3f080168ade6a503b/crates/anybuild/src/build/docker.rs#L284-L316
[mcp-main]: https://github.com/wasmerio/anybuild/blob/fd81bd8ed779b79b4ca639c3f080168ade6a503b/examples/python-mcp/main.py#L8
[chatgpt-main]: https://github.com/wasmerio/anybuild/blob/fd81bd8ed779b79b4ca639c3f080168ade6a503b/examples/python-mcp-chatgpt/main.py#L3
[mcp-v2]: https://py.sdk.modelcontextprotocol.io/v2/migration/
[mcp-releases]: https://pypi.org/project/mcp/#history
[wasix-core]: https://python-registry.wasix.org/simple/pydantic-core/
[cross-wheels]: https://github.com/wasmerio/anybuild/blob/fd81bd8ed779b79b4ca639c3f080168ade6a503b/crates/anybuild/resources/starlib/tools/python.bzl#L73-L100
[uv-extras]: https://docs.astral.sh/uv/pip/compatibility/#pip-compile-defaults
[wasmer-version]: https://github.com/wasmerio/anybuild/blob/fd81bd8ed779b79b4ca639c3f080168ade6a503b/crates/anybuild/src/run/wasmer.rs#L458-L478
[wasmer-run]: https://github.com/wasmerio/anybuild/blob/fd81bd8ed779b79b4ca639c3f080168ade6a503b/crates/anybuild/src/run/wasmer.rs#L1211-L1272
[python-env]: https://github.com/wasmerio/anybuild/blob/fd81bd8ed779b79b4ca639c3f080168ade6a503b/crates/anybuild/resources/starlib/tools/python.bzl#L172-L186
[mcp-command]: https://github.com/wasmerio/anybuild/blob/fd81bd8ed779b79b4ca639c3f080168ade6a503b/crates/anybuild/src/providers/python.rs#L577-L587
[cli-main]: https://github.com/wasmerio/anybuild/blob/fd81bd8ed779b79b4ca639c3f080168ade6a503b/crates/anybuild-cli/src/main.rs#L126-L149
[blog]: https://wasmer.io/posts/anybuild-build-anything-deploy-anywhere
[wasmer-deploy]: https://github.com/wasmerio/anybuild/blob/fd81bd8ed779b79b4ca639c3f080168ade6a503b/crates/anybuild/src/deploy/wasmer.rs#L88-L126
[goldens]: https://github.com/wasmerio/anybuild/blob/fd81bd8ed779b79b4ca639c3f080168ade6a503b/crates/anybuild-cli/tests/goldens.rs#L1-L114
