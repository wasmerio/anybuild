'use strict';

const BACKEND = { owner: 'wasmerio', repo: 'backend' };
const VERSION = String.raw`(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-[\da-zA-Z.-]+)?(?:\+[\da-zA-Z.-]+)?`;
const BACKEND_FILES = [
  '.github/workflows/qa.yaml',
  'rust/Dockerfile',
  'rust/env.local',
  'rust/test-images/e2e-test-base-image.Dockerfile',
];
const RELEASE_ASSETS = [
  'anybuild-aarch64-apple-darwin.tar.gz',
  'anybuild-x86_64-apple-darwin.tar.gz',
  'anybuild-aarch64-unknown-linux-musl.tar.gz',
  'anybuild-x86_64-unknown-linux-musl.tar.gz',
  'anybuild-x86_64-pc-windows-msvc.zip',
  'SHA256SUMS',
];

function parseVersion(version) {
  if (!new RegExp(`^${VERSION}$`).test(version)) {
    throw new Error(`Invalid Anybuild version: ${version}`);
  }
  const [core, prerelease = ''] = version.split('+')[0].split(/-(.*)/s);
  const identifiers = prerelease ? prerelease.split('.') : [];
  if (identifiers.some((part) => !part || /^0\d+$/.test(part))) {
    throw new Error(`Invalid Anybuild prerelease: ${version}`);
  }
  return { core: core.split('.').map(BigInt), identifiers };
}

function compareVersions(left, right) {
  const a = parseVersion(left);
  const b = parseVersion(right);
  for (let index = 0; index < 3; index += 1) {
    if (a.core[index] !== b.core[index]) {
      return a.core[index] > b.core[index] ? 1 : -1;
    }
  }
  if (!a.identifiers.length || !b.identifiers.length) {
    return Math.sign(Number(!a.identifiers.length) -
      Number(!b.identifiers.length));
  }
  for (let index = 0; index < Math.max(
    a.identifiers.length, b.identifiers.length,
  ); index += 1) {
    const x = a.identifiers[index];
    const y = b.identifiers[index];
    if (x === y) continue;
    if (x === undefined) return -1;
    if (y === undefined) return 1;
    const xNumeric = /^\d+$/.test(x);
    const yNumeric = /^\d+$/.test(y);
    if (xNumeric && yNumeric) return BigInt(x) > BigInt(y) ? 1 : -1;
    if (xNumeric !== yNumeric) return xNumeric ? -1 : 1;
    return x > y ? 1 : -1;
  }
  return 0;
}

function validateRelease(release, tag) {
  if (release.tag_name !== tag || !tag.startsWith('v')) {
    throw new Error(`Expected a v-prefixed release tag, got ${tag}`);
  }
  const version = tag.slice(1);
  parseVersion(version);
  if (release.draft || !release.published_at) {
    throw new Error(`${tag} has not been published`);
  }
  const missing = RELEASE_ASSETS.filter((name) => !release.assets.some(
    (asset) => asset.name === name && asset.size > 0 &&
      asset.state === 'uploaded',
  ));
  if (missing.length) {
    throw new Error(`${tag} is missing completed assets: ${missing.join(', ')}`);
  }
  return { tag, version, releaseUrl: release.html_url };
}

function updateVersionPin(content, version, path) {
  parseVersion(version);
  const pin = new RegExp(
    `^([ \\t]*(?:ENV[ \\t]+)?ANYBUILD_VERSION(?:[ \\t]*=[ \\t]*|:[ \\t]*))` +
      `(["']?)(${VERSION})(\\2)([ \\t]*(?:#.*)?\\r?)$`,
    'gm',
  );
  const matches = [...content.matchAll(pin)];
  if (matches.length !== 1) {
    throw new Error(`Expected one ANYBUILD_VERSION pin in ${path}, ` +
      `found ${matches.length}`);
  }
  return {
    previous: matches[0][3],
    content: content.replace(pin, (_, prefix, quote, old, close, suffix) =>
      `${prefix}${quote}${version}${close}${suffix}`),
  };
}

async function createBackendReleasePr(github, release) {
  const { tag, version, releaseUrl } = release;
  parseVersion(version);
  if (tag !== `v${version}`) throw new Error('Release tag/version mismatch');
  const branch = `chore/bump-anybuild-${tag}`;
  const { data: repository } = await github.rest.repos.get(BACKEND);
  const base = repository.default_branch;
  const { data: existing } = await github.rest.pulls.list({
    ...BACKEND, state: 'all', head: `${BACKEND.owner}:${branch}`, base,
  });
  // Closed and merged PRs count too: retries must not reopen a rejected bump.
  if (existing.length) {
    return { status: 'existing', url: existing[0].html_url };
  }

  const { data: baseRef } = await github.rest.git.getRef({
    ...BACKEND, ref: `heads/${base}`,
  });
  let branchRef;
  try {
    ({ data: branchRef } = await github.rest.git.getRef({
      ...BACKEND, ref: `heads/${branch}`,
    }));
  } catch (error) {
    if (error.status !== 404) throw error;
  }
  const parent = branchRef ? branchRef.object.sha : baseRef.object.sha;
  const { data: commit } = await github.rest.git.getCommit({
    ...BACKEND, commit_sha: parent,
  });
  const tree = [];
  for (const path of BACKEND_FILES) {
    if (branchRef) {
      const { data: baseFile } = await github.rest.repos.getContent({
        ...BACKEND, path, ref: baseRef.object.sha,
      });
      const current = Buffer.from(baseFile.content, 'base64').toString('utf8');
      if (compareVersions(updateVersionPin(current, version, path).previous,
        version) > 0) {
        return { status: 'superseded' };
      }
    }
    const { data: file } = await github.rest.repos.getContent({
      ...BACKEND, path, ref: parent,
    });
    if (file.type !== 'file' || file.encoding !== 'base64') {
      throw new Error(`Expected a base64-encoded backend file: ${path}`);
    }
    const content = Buffer.from(file.content, 'base64').toString('utf8');
    const updated = updateVersionPin(content, version, path);
    if (compareVersions(updated.previous, version) > 0) {
      return { status: 'superseded' };
    }
    if (updated.content !== content) {
      tree.push({ path, mode: '100644', type: 'blob', content: updated.content });
    }
  }
  if (!tree.length && (!branchRef || parent === baseRef.object.sha)) {
    return { status: 'up-to-date' };
  }

  if (tree.length) {
    const { data: newTree } = await github.rest.git.createTree({
      ...BACKEND, base_tree: commit.tree.sha, tree,
    });
    const { data: newCommit } = await github.rest.git.createCommit({
      ...BACKEND, message: `chore: bump anybuild to ${version}`,
      tree: newTree.sha, parents: [parent],
    });
    if (branchRef) {
      await github.rest.git.updateRef({
        ...BACKEND, ref: `heads/${branch}`, sha: newCommit.sha, force: false,
      });
    } else {
      await github.rest.git.createRef({
        ...BACKEND, ref: `refs/heads/${branch}`, sha: newCommit.sha,
      });
    }
  }

  const { data: pr } = await github.rest.pulls.create({
    ...BACKEND, base, head: branch,
    title: `chore: bump anybuild to ${version}`,
    body: `Update the backend's Anybuild version pins to ${version}.\n\n` +
      `Release: ${releaseUrl}\n\n` +
      'All five platform binaries and SHA256SUMS are attached to the ' +
      'published release.\n\n' +
      BACKEND_FILES.map((path) => `- \`${path}\``).join('\n'),
  });
  return { status: 'created', url: pr.html_url };
}

module.exports = {
  BACKEND_FILES, RELEASE_ASSETS, compareVersions, validateRelease,
  updateVersionPin, createBackendReleasePr,
};
