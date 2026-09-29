# Backend release updates

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

## Credential setup

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

## Retries and backfills

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

[gh-token]: https://docs.github.com/en/actions/concepts/security/github_token
