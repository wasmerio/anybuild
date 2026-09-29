'use strict';

const assert = require('node:assert/strict');
const test = require('node:test');
const {
  BACKEND_FILES, RELEASE_ASSETS, compareVersions, validateRelease,
  updateVersionPin, createBackendReleasePr,
} = require('./backend-release-pr.cjs');

const RELEASE = {
  tag: 'v0.30.0', version: '0.30.0',
  releaseUrl: 'https://github.com/wasmerio/anybuild/releases/tag/v0.30.0',
};
const BRANCH = 'heads/chore/bump-anybuild-v0.30.0';

function files(version = '0.29.0') {
  return {
    '.github/workflows/qa.yaml':
      `        env:\n          ANYBUILD_VERSION: "${version}"\n` +
      '        run: curl -fsSL https://anybuild.run/install | sh\n',
    'rust/Dockerfile':
      `ENV SHELL=/bin/sh\nENV ANYBUILD_VERSION=${version}\nRUN true\n`,
    'rust/env.local':
      `RUST_LOG=info\nANYBUILD_VERSION=${version}\nOTHER_VERSION=1.2.3\n`,
    'rust/test-images/e2e-test-base-image.Dockerfile':
      `FROM debian\nENV ANYBUILD_VERSION=${version}\nRUN true\n`,
  };
}

function fakeGithub(options = {}) {
  const snapshots = new Map([['base', options.files || files()]]);
  const refs = new Map([['heads/main', 'base']]);
  const prs = options.prs || [];
  const writes = [];
  const trees = new Map();
  let failPr = options.failPr || false;
  if (options.branchFiles) {
    snapshots.set('branch', options.branchFiles);
    refs.set(BRANCH, 'branch');
  }
  const github = { rest: {
    repos: {
      get: async () => ({ data: { default_branch: 'main' } }),
      getContent: async ({ path, ref }) => ({ data: {
        type: 'file', encoding: 'base64',
        content: Buffer.from(snapshots.get(ref)[path]).toString('base64'),
      } }),
    },
    pulls: {
      list: async ({ state, head, base }) => {
        assert.equal(state, 'all');
        assert.equal(head, 'wasmerio:chore/bump-anybuild-v0.30.0');
        assert.equal(base, 'main');
        return { data: prs };
      },
      create: async (input) => {
        writes.push(['pr', input]);
        if (failPr) {
          failPr = false;
          throw new Error('Temporary PR creation failure');
        }
        const pr = { html_url: 'https://github.com/wasmerio/backend/pull/9999' };
        prs.push(pr);
        return { data: pr };
      },
    },
    git: {
      getRef: async ({ ref }) => {
        if (!refs.has(ref)) throw Object.assign(new Error('Not found'), {
          status: 404,
        });
        return { data: { object: { sha: refs.get(ref) } } };
      },
      getCommit: async ({ commit_sha }) => ({ data: {
        tree: { sha: commit_sha },
      } }),
      createTree: async (input) => {
        writes.push(['tree', input]);
        const contents = { ...snapshots.get(input.base_tree) };
        for (const entry of input.tree) contents[entry.path] = entry.content;
        const sha = `tree-${trees.size}`;
        trees.set(sha, contents);
        return { data: { sha } };
      },
      createCommit: async (input) => {
        writes.push(['commit', input]);
        const sha = `commit-${snapshots.size}`;
        assert.ok(snapshots.has(input.parents[0]));
        snapshots.set(sha, trees.get(input.tree));
        return { data: { sha } };
      },
      createRef: async (input) => {
        writes.push(['ref', input]);
        refs.set(input.ref.replace(/^refs\//, ''), input.sha);
      },
      updateRef: async (input) => {
        writes.push(['ref', input]);
        assert.equal(input.force, false);
        refs.set(input.ref, input.sha);
      },
    },
  } };
  return { github, writes, prs, refs, snapshots };
}

test('requires a published release and all completed binary assets', () => {
  const release = {
    tag_name: RELEASE.tag, draft: false, published_at: '2026-09-29T12:00:00Z',
    html_url: RELEASE.releaseUrl,
    assets: RELEASE_ASSETS.map((name) => ({ name, size: 100, state: 'uploaded' })),
  };
  assert.deepEqual(validateRelease(release, RELEASE.tag), RELEASE);
  assert.throws(() => validateRelease({ ...release, draft: true }, RELEASE.tag));
  assert.throws(() => validateRelease({ ...release, published_at: null },
    RELEASE.tag));
  for (const name of RELEASE_ASSETS) {
    for (const patch of [{ size: 0 }, { state: 'new' }]) {
      const incomplete = release.assets.map((asset) => asset.name === name ?
        { ...asset, ...patch } : asset);
      assert.throws(() => validateRelease({ ...release, assets: incomplete },
        RELEASE.tag), /missing completed assets/);
    }
    assert.throws(() => validateRelease({ ...release,
      assets: release.assets.filter((asset) => asset.name !== name),
    }, RELEASE.tag), /missing completed assets/);
  }
  assert.throws(() => validateRelease({ ...release, tag_name: 'vbad' }, 'vbad'));
});

test('updates all four backend pins in one commit and links the release',
  async () => {
    const state = fakeGithub();
    const result = await createBackendReleasePr(state.github, RELEASE);
    assert.equal(result.status, 'created');
    const contents = state.snapshots.get(state.refs.get(BRANCH));
    assert.deepEqual(contents, files(RELEASE.version));
    const tree = state.writes.find(([kind]) => kind === 'tree')[1];
    assert.deepEqual(tree.tree.map((entry) => entry.path), BACKEND_FILES);
    const commits = state.writes.filter(([kind]) => kind === 'commit');
    assert.equal(commits.length, 1);
    assert.deepEqual(commits[0][1].parents, ['base']);
    const pr = state.writes.find(([kind]) => kind === 'pr')[1];
    assert.equal(pr.base, 'main');
    assert.equal(pr.head, 'chore/bump-anybuild-v0.30.0');
    assert.ok(pr.body.includes(RELEASE.releaseUrl));
  });

test('reruns reuse existing open, closed, and merged PRs without writes',
  async () => {
    for (const status of ['open', 'closed', 'merged']) {
      const pr = { state: status,
        html_url: 'https://github.com/wasmerio/backend/pull/9999' };
      const state = fakeGithub({ prs: [pr] });
      const result = await createBackendReleasePr(state.github, RELEASE);
      assert.deepEqual(result, { status: 'existing', url: pr.html_url });
      assert.equal(state.writes.length, 0);
    }
  });

test('recovers PR creation failure without another commit or duplicate PR',
  async () => {
    const state = fakeGithub({ failPr: true });
    await assert.rejects(createBackendReleasePr(state.github, RELEASE),
      /Temporary PR creation failure/);
    const writesBeforeRetry = state.writes.length;
    assert.equal((await createBackendReleasePr(state.github, RELEASE)).status,
      'created');
    assert.deepEqual(state.writes.slice(writesBeforeRetry).map(([kind]) => kind),
      ['pr']);
    await createBackendReleasePr(state.github, RELEASE);
    assert.equal(state.prs.length, 1);
  });

test('completes an unfinished bump on its existing branch without force pushing',
  async () => {
    const contents = files();
    contents['rust/Dockerfile'] = files(RELEASE.version)['rust/Dockerfile'];
    const state = fakeGithub({ branchFiles: contents });
    const result = await createBackendReleasePr(state.github, RELEASE);
    assert.equal(result.status, 'created');
    assert.deepEqual(state.snapshots.get(state.refs.get(BRANCH)),
      files(RELEASE.version));
    const commit = state.writes.find(([kind]) => kind === 'commit')[1];
    assert.deepEqual(commit.parents, ['branch']);
    const update = state.writes.find(([kind]) => kind === 'ref')[1];
    assert.equal(update.ref, BRANCH);
    assert.equal(update.force, false);
  });

test('skips an already pinned release and prevents downgrades, including retries',
  async () => {
    for (const options of [
      { files: files('0.30.0') },
      { files: files('0.31.0') },
      { files: files('0.31.0'), branchFiles: files('0.30.0') },
    ]) {
      const state = fakeGithub(options);
      const result = await createBackendReleasePr(state.github, RELEASE);
      assert.ok(['up-to-date', 'superseded'].includes(result.status));
      assert.equal(state.writes.length, 0);
    }
  });

test('missing or ambiguous pins fail before committing any changes', async () => {
  for (const content of ['RUST_LOG=info\n',
    'ANYBUILD_VERSION=0.28.0\nANYBUILD_VERSION=0.29.0\n']) {
    const contents = files();
    contents['rust/env.local'] = content;
    const state = fakeGithub({ files: contents });
    await assert.rejects(createBackendReleasePr(state.github, RELEASE),
      /Expected one ANYBUILD_VERSION pin/);
    assert.equal(state.writes.length, 0);
  }
});

test('updates prerelease pins while preserving quotes, comments, and CRLF', () => {
  const original = "ANYBUILD_VERSION = '0.30.0-rc.1' # test\r\n";
  const updated = updateVersionPin(original, '0.30.0-rc.2', 'test');
  assert.equal(updated.content,
    "ANYBUILD_VERSION = '0.30.0-rc.2' # test\r\n");
  assert.ok(compareVersions('0.30.0', '0.30.0-rc.2') > 0);
  assert.ok(compareVersions('0.30.0-rc.10', '0.30.0-rc.2') > 0);
  assert.ok(compareVersions('0.30.0-beta', '0.30.0-rc') < 0);
  assert.equal(compareVersions('0.30.0+build.1', '0.30.0+build.2'), 0);
  assert.throws(() => updateVersionPin(original, '0.30.0-rc.01', 'test'));
  assert.throws(() => updateVersionPin(original, '0.30.0\nOTHER=oops', 'test'));
});
