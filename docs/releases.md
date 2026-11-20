# Releases

## Crates.io publishing

Release Please coordinates one version for the workspace, `anybuild`, and
`anybuild-cli`, including the CLI's dependency on the SDK. Release Artifacts
builds the binaries and publishes the GitHub release, then calls Publish
Crates with the same release commit. The SDK is published before the CLI.

Publish Crates verifies the published release tag, commit, package versions,
and SDK dependency before building the crate archives. It obtains a temporary
crates.io token using `rust-lang/crates-io-auth-action`; no crates.io token
secret is needed in GitHub. Only runs on `wasmerio/anybuild`'s `main` branch
can publish, using the `crates-io` GitHub environment.

### Initial publish and trusted publisher setup

Both crates must exist before crates.io permits trusted publisher setup.
For the first publish, use a clean checkout of the release tag and a local
crates.io API token with permission to create both crates:

```sh
git checkout v0.34.0
cargo publish --locked --registry crates-io --dry-run \
  -p anybuild -p anybuild-cli
cargo publish --locked --registry crates-io -p anybuild
cargo publish --locked --registry crates-io -p anybuild-cli
```

Create the GitHub environment `crates-io` with a deployment branch policy
allowing only `main`. In each crate's Settings → Trusted Publishing, add
GitHub configurations with owner `wasmerio`, repository
`anybuild`, and environment `crates-io` for these workflow filenames:

- `release-please.yml`: automatic releases from merged release PRs.
- `release-artifacts.yml`: manually dispatched artifact builds.
- `publish-crates.yml`: manual crate publication retries and backfills.

Crates.io checks the calling workflow's OIDC `workflow_ref`, so all three
entry points need a configuration even though they share Publish Crates.
The `id-token: write` permission is passed through each reusable workflow
call and granted to the publishing job. See the
[crates.io trusted publishing documentation][trusted-publishing].

### Crate retries and backfills

Run Publish Crates manually from `main`, supplying a published tag such as
`v0.34.0`. The workflow uses current publishing automation with the source
from that release tag, so releases predating the workflow are supported.
Existing versions are skipped, allowing a retry after only the SDK was
published. A yanked version or a registry failure stops publication.

Run the release planning tests locally with:

```sh
python3 -m unittest discover -s scripts -p test_crate_release.py
```

## Backend release updates

After Release Artifacts uploads all platform binaries and `SHA256SUMS` and
publishes the release, it calls Update Backend Anybuild. That workflow
verifies the published release and creates a PR in `wasmerio/backend` against
its default branch, updating these version pins:

- `.github/workflows/qa.yaml`
- `rust/Dockerfile`
- `rust/env.local`
- `rust/test-images/e2e-test-base-image.Dockerfile`

The workflow also handles manually published GitHub releases. The direct
call from Release Artifacts is necessary because releases published with
`GITHUB_TOKEN` do not trigger another release-event workflow. See
[GitHub's token documentation][gh-token].

### Credential setup

Add the Actions secret `BACKEND_PR_TOKEN` to `wasmerio/anybuild`, or expose an
organization secret with that name to the repository. Use a fine-grained
personal access token restricted to `wasmerio/backend` with these repository
permissions:

- Contents: read and write.
- Pull requests: read and write.
- Workflows: write, because the PR changes `qa.yaml`.

The ordinary Anybuild `GITHUB_TOKEN` cannot write to the backend repository.
The backend credential is used only for the backend API calls; release
validation uses Anybuild's own token. No backend code is executed.

### Retries and backfills

Run Update Backend Anybuild manually and supply the release tag, for example
`v0.29.0`. This also recovers a release whose binaries were attached after
publication or whose first update attempt failed because the credential was
missing.

Each version uses the branch `chore/bump-anybuild-vX.Y.Z`. An existing open,
closed, or merged PR for that branch prevents a duplicate PR. If branch
creation succeeded but PR creation failed, retrying opens the PR from that
branch. The workflow skips versions already pinned in the backend and never
downgrades a newer pin. Missing or ambiguous pins fail before backend writes.

Run the automation tests locally with:

```sh
node --test scripts/backend-release-pr.test.cjs
```

## Lambda adapter mirror

Docker runtime images default to the upstream ECR Public image. Docker
E2E tests in CI set `ANYBUILD_LAMBDA_ADAPTER_IMAGE` to the
`ghcr.io/wasmerio/aws-lambda-adapter:1.0.0` mirror, pinned to the upstream
multi-architecture digest. The Mirror Lambda Adapter workflow copies the
upstream image without rebuilding it, preserves AMD64 and ARM64, and checks
that the mirrored digest matches. It uses the repository's `GITHUB_TOKEN`
with `packages: write`; no extra publishing credential is required.
Docker test jobs authenticate with `GITHUB_TOKEN` and `packages: read`, so
they can also pull the mirror while the package is private.

To upgrade, update `ADAPTER_VERSION` and `ADAPTER_DIGEST` in
`.github/workflows/mirror-lambda-adapter.yml`, publish the mirror, and update
the image override in `.github/workflows/reusable-test-e2e.yml` and the
default version in `crates/anybuild/src/run/docker.rs`. Run the workflow
manually to retry a publication. Existing tags are never overwritten.
For unauthenticated local pulls, set the package visibility to Public in
package settings and verify an unauthenticated pull.

[gh-token]: https://docs.github.com/en/actions/concepts/security/github_token
[trusted-publishing]: https://crates.io/docs/trusted-publishing
