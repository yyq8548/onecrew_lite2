import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, writeFile, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { packageWorkspace } from './package-workspace.mjs';
import { targets, collectRelease, selectBundle, verifyBundle, nativeTarget, sourceIdentity } from './workspace-release.mjs';

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const writeJson = (file, data) => writeFile(file, JSON.stringify(data));
async function fixture(base, host, commit = 'a'.repeat(40)) {
  const cli = path.join(base, `cli-${host}`), library = path.join(base, `library-${host}`);
  await mkdir(cli); await mkdir(library);
  const name = host.startsWith('win32') ? 'onecrew_lite.exe' : 'onecrew_lite';
  const bytes = Buffer.from(`fixture-${host}`);
  const integrity = { size: bytes.length, sha256: hash(bytes) };
  await writeFile(path.join(cli, name), bytes); await writeFile(path.join(library, name), bytes);
  await writeJson(path.join(cli, 'manifest.json'), { schema_version: 1, product: 'onecrew_lite-cli',
    version: '1.2.3', target: targets[host], ...integrity });
  await writeJson(path.join(library, 'manifest.json'), { version: '0.1.4', target: host,
    capabilities: ['discovery-contacts-v2', 'recommendation-capture-v1'], files: [{ path: name, ...integrity }] });
  const directory = path.join(base, 'bundles', host);
  await packageWorkspace(cli, library, directory);
  await writeJson(path.join(directory, 'release-build.json'), { schema_version: 1, commit,
    version: '1.2.3', host, target: targets[host], node: 'v26.7.0', plugin: 'embedded-in-cli',
    manifest_sha256: hash(await readFile(path.join(directory, 'manifest.json'))) });
  return directory;
}

test('collects three native bundles, selects whole pair and detects tampering', async () => {
  const temp = await mkdtemp(path.join(os.tmpdir(), 'onecrew-release-'));
  try {
    const bundles = path.join(temp, 'bundles'); await mkdir(bundles);
    for (const host of Object.keys(targets)) await fixture(temp, host);
    const index = path.join(bundles, 'release-manifest.json');
    const result = await collectRelease(bundles, index);
    assert.equal(result.artifacts.length, 3);
    assert.equal(result.acceptance, 'pending-native-runtime');
    for (const host of Object.keys(targets)) {
      const [platform, arch] = host.split('-');
      assert.equal((await selectBundle(bundles, platform, arch)).host, host);
    }
    await assert.rejects(collectRelease(bundles, index), /EEXIST/);
    assert.throws(() => nativeTarget('linux', 'x64'), /Unsupported/);
    const dir = path.join(bundles, 'win32-x64');
    await writeFile(path.join(dir, 'onecrew_lite.exe'), 'tampered');
    await assert.rejects(verifyBundle(dir), /integrity/);
  } finally { await rm(temp, { recursive: true, force: true }); }
});

test('rejects incomplete, mixed-commit and unexpected-file releases', async () => {
  const temp = await mkdtemp(path.join(os.tmpdir(), 'onecrew-release-'));
  try {
    const bundles = path.join(temp, 'bundles'); await mkdir(bundles);
    await fixture(temp, 'win32-x64');
    await assert.rejects(collectRelease(bundles, path.join(temp, 'index.json')), /ENOENT/);
    await fixture(temp, 'darwin-arm64');
    const dir = await fixture(temp, 'darwin-x64', 'b'.repeat(40));
    await assert.rejects(collectRelease(bundles, path.join(temp, 'index.json')), /Mixed source/);
    await writeFile(path.join(dir, 'private-data.txt'), 'must not ship');
    await assert.rejects(verifyBundle(dir), /Unexpected/);
  } finally { await rm(temp, { recursive: true, force: true }); }
});

test('source gate rejects tracked and untracked changes', async () => {
  const temp = await mkdtemp(path.join(os.tmpdir(), 'onecrew-source-'));
  try {
    const git = args => execFileSync('git', args, { cwd: temp, stdio: 'pipe', windowsHide: true });
    git(['init']); await writeFile(path.join(temp, 'source'), 'one'); git(['add', 'source']);
    git(['-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-m', 'fixture']);
    assert.match(sourceIdentity(temp), /^[a-f0-9]{40}$/);
    await writeFile(path.join(temp, 'source'), 'two');
    assert.throws(() => sourceIdentity(temp), /clean repository/);
    await writeFile(path.join(temp, 'source'), 'one');
    await writeFile(path.join(temp, 'extra'), 'untracked');
    assert.throws(() => sourceIdentity(temp), /clean repository/);
  } finally { await rm(temp, { recursive: true, force: true }); }
});

test('rejects unsafe manifest paths and incompatible Library even with recomputed outer hashes', async () => {
  const temp = await mkdtemp(path.join(os.tmpdir(), 'onecrew-release-'));
  try {
    await mkdir(path.join(temp, 'bundles'));
    const dir = await fixture(temp, 'win32-x64');
    const manifestPath = path.join(dir, 'manifest.json');
    const receiptPath = path.join(dir, 'release-build.json');
    const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
    const receipt = JSON.parse(await readFile(receiptPath, 'utf8'));
    const refresh = async () => {
      await writeJson(manifestPath, manifest);
      receipt.manifest_sha256 = hash(await readFile(manifestPath));
      await writeJson(receiptPath, receipt);
    };
    manifest.files.push({ path: '../outside', size: 1, sha256: 'a'.repeat(64) });
    await refresh();
    await assert.rejects(verifyBundle(dir), /Unsafe/);
    manifest.files.pop();
    const libraryPath = path.join(dir, 'onecrew_lite_library/manifest.json');
    const library = JSON.parse(await readFile(libraryPath, 'utf8'));
    library.capabilities = [];
    await writeJson(libraryPath, library);
    const bytes = await readFile(libraryPath);
    Object.assign(manifest.files.find(file => file.path === 'onecrew_lite_library/manifest.json'),
      { size: bytes.length, sha256: hash(bytes) });
    await refresh();
    await assert.rejects(verifyBundle(dir), /Incompatible/);
  } finally { await rm(temp, { recursive: true, force: true }); }
});
