import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { resolve, join, basename } from 'node:path';
import { parseArgs } from 'node:util';
import { readDevelopmentBuildIdentity } from '../onecrew_lite-app/src/service/build-identity.ts';

const { values } = parseArgs({ options: {
  'candidate-zip': { type: 'string' }, 'source-commit': { type: 'string' },
  output: { type: 'string' }, revision: { type: 'string' },
  'product-version': { type: 'string' }, 'allow-dirty-candidate': { type: 'boolean' },
}, strict: true });
assert(values['candidate-zip'] && values['source-commit'] && values.output && values.revision && values['product-version']);
assert.match(values['source-commit'], /^[a-f0-9]{40}$/u);
assert.match(values.revision, /^R[1-9][0-9]*$/u);
assert.equal(execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(), values['source-commit']);
const dirty = execFileSync('git', ['status', '--porcelain'], { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 }).trim().length > 0;
assert(!dirty || values['allow-dirty-candidate'] === true, 'Dirty source requires explicit local-candidate permission');
const require = createRequire(resolve('onecrew_lite-app/package.json'));
const JSZip = require('jszip');
const sha = (bytes: Uint8Array) => createHash('sha256').update(bytes).digest('hex');
const source = await JSZip.loadAsync(await readFile(values['candidate-zip']), { checkCRC32: true });
const candidate = JSON.parse(await source.file('candidate.json').async('string'));
const manifest = JSON.parse(await source.file('runtime-manifest.json').async('string'));
const identity = await readDevelopmentBuildIdentity();
assert.equal(candidate.productVersion, values['product-version']);
assert.equal(candidate.target, 'darwin-arm64');
assert.equal(candidate.productionReady, false);
assert.equal(manifest.productVersion, candidate.productVersion);
assert.equal(manifest.source.checkoutCommit, values['source-commit']);
assert.equal(manifest.source.dirty, dirty);
assert.equal(manifest.source.sourceTreeSha256, identity.source_tree_sha256);
assert.equal(manifest.lockfileSha256, identity.dependency_lock_sha256);
assert.equal(sha(await source.file('onecrew_lite').async('nodebuffer')), candidate.executableSha256);
const prefix = 'OneCrew-Lite/';
const zip = new JSZip();
const checksums = new Map<string, string>();
function include(name: string, bytes: Buffer, executable = false) {
  assert(!checksums.has(name));
  assert(!name.startsWith('/') && !name.split('/').includes('..') && !/[\r\n\\]/u.test(name));
  checksums.set(name, sha(bytes));
  zip.file(prefix + name, bytes, { unixPermissions: executable ? 0o100755 : 0o100644 });
}
for (const entry of Object.values(source.files) as { name: string; dir: boolean; async(kind: string): Promise<Buffer> }[]) {
  if (entry.dir) continue;
  assert(/^(onecrew_lite|candidate\.json|runtime-manifest\.json|licenses\/node\.txt|licenses\/[a-f0-9]+\/[^/]+)$/u.test(entry.name), entry.name);
  include(entry.name, await entry.async('nodebuffer'), entry.name === 'onecrew_lite');
}
for (const name of ['README.md', 'INSTALL-CODEX.md', 'CODEX-PROMPT.md', 'RELEASE-STATUS.md']) {
  const bytes = await readFile(join('mac_installer', name));
  assert(bytes.toString('utf8').includes(candidate.productVersion), name);
  include(name, bytes);
}
include('SHA256SUMS', Buffer.from([...checksums].map(([name, hash]) => `${hash}  ${name}\n`).join('')));
include('SHA256SUMS.json', Buffer.from(JSON.stringify(Object.fromEntries(checksums), null, 2) + '\n'));
const bytes = await zip.generateAsync({ type: 'nodebuffer', compression: 'DEFLATE', compressionOptions: { level: 6 }, platform: 'UNIX' });
const verified = await JSZip.loadAsync(bytes, { checkCRC32: true });
assert.equal(Object.values(verified.files).filter((entry: { dir: boolean }) => !entry.dir).length, checksums.size);
for (const [name, hash] of checksums) assert.equal(sha(await verified.file(prefix + name).async('nodebuffer')), hash, name);
assert.equal(verified.file(prefix + 'onecrew_lite').unixPermissions & 0o111, 0o111);
const output = resolve(values.output);
await mkdir(resolve(output, '..'), { recursive: true });
await writeFile(output, bytes, { flag: 'wx' });
const receipt = {
  product: 'onecrew_lite', version: candidate.productVersion, revision: values.revision, target: candidate.target,
  sourceCommit: values['source-commit'], sourceTreeSha256: identity.source_tree_sha256,
  sourceDirty: dirty,
  dependencyLockSha256: identity.dependency_lock_sha256,
  archive: basename(output), bytes: bytes.length, sha256: sha(bytes),
  executableSha256: candidate.executableSha256, executableBytes: candidate.executableBytes,
  payloadSha256: candidate.payloadSha256, nodeVersion: manifest.nodeVersion,
  installationGuide: prefix + 'INSTALL-CODEX.md', verifiedFiles: checksums.size,
  companionPlugin: { versionPrefix: `${candidate.productVersion}+codex.`, expectedVersion: 'Generated and verified by this binary on the target computer; suffix binds its paths and data directory.' },
  signature: 'ad-hoc', notarized: false, productionReady: false,
  licenseTextMissing: candidate.licenseTextMissing,
  scope: 'Mac ARM64 customer ZIP; Windows and production distribution are unchanged.',
};
await writeFile(output + '.sha256', `${receipt.sha256}  ${basename(output)}\n`, { flag: 'wx' });
await writeFile(output + '.release.json', JSON.stringify(receipt, null, 2) + '\n', { flag: 'wx' });
console.log(JSON.stringify(receipt));
