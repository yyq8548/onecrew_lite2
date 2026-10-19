import assert from 'node:assert/strict';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { resolve, join, basename } from 'node:path';
import { createRequire } from 'node:module';
import { createHash } from 'node:crypto';

const require = createRequire(resolve('onecrew_lite-app/package.json'));
const JSZip = require('jszip');
const candidate = resolve('.scratch/unified-sea-candidate-87');
const revision = 'R8.1';
const output = resolve(`dist/OneCrew-Lite-0.7.1-Windows-x64-${revision}.zip`);
const sha = (bytes: Uint8Array) => createHash('sha256').update(bytes).digest('hex');
const receipt = JSON.parse(await readFile(join(candidate, 'candidate.json'), 'utf8'));
assert.equal(receipt.target, 'win32-x64');
assert.equal(receipt.executableSha256, '1238fa16671a7bf44c374fae9b8b55743ab6238aa84fe4c274cd01a885b2cb64');
assert.equal(receipt.payloadSha256, '7cc9326c84d3fb25e46db6b939bcaa197b08cfc12b4933b1a74c7ce25ad36b24');
for (const name of ['windows75-encoding-owner', 'windows87-native', 'windows87-checks', 'windows79-launcher', 'windows79-discovery', 'windows78-existing-browser', 'windows81-planning-checks']) {
  const log = await readFile(resolve(`.scratch/${name}.log`), 'utf8');
  assert.match(log, /fail 0/, name);
  assert.doesNotMatch(log, /fail [1-9]/, name);
}
const pages = JSON.parse(await readFile(resolve('.scratch/windows87-pages.json'), 'utf8'));
assert.equal(pages.stableInstance, true);
assert.equal(pages.idleSyncUnchanged, true);
assert.equal(pages.savedTable.table_id, 'tblSDyju03ykNKPSYCu');
assert.ok(pages.receipts.length >= 20);
assert.ok(pages.receipts.every((row: { status: number; ms: number }) => row.status === 200 && row.ms < 3000));
const planning = JSON.parse(await readFile(resolve('.scratch/windows81-planning-http.json'), 'utf8'));
const recommendation = JSON.parse(await readFile(resolve('.scratch/windows85-installed-recommendation.json'), 'utf8'));
assert.equal(recommendation.stableInstance, true);
assert.equal(recommendation.counts.qualified, 1);
assert.equal(recommendation.candidate[0].platform_account_id, 'UCyUZg_cPAg1-QjPHKJu20ZA');
assert.equal(recommendation.candidate[0].evidence.assessment.match_rating.level, 'good');
const retained = JSON.parse(await readFile(resolve('.scratch/windows83-retained.json'), 'utf8'));
assert.equal(retained.length, 11);
assert.ok(retained.every((row: { retained: boolean }) => row.retained));
assert.equal(planning.browserSessionStable, true);
assert.equal(planning.savedPlanReadable, true);
assert.equal(planning.stableInstance, true);
assert.ok(planning.samples.length >= 12);
assert.equal(planning.plan.target_count, 15);
assert.equal(planning.plan.criteria.min_subscribers, 2000);
assert.deepEqual(planning.plan.scopes.map((scope: { platform: string }) => scope.platform), ['youtube']);
for (const mode of ['Normal', 'Elevated']) {
  const validation = JSON.parse((await readFile(resolve(`.scratch/windows73-${mode}-native-result.json`), 'utf8')).replace(/^\uFEFF/, ''));
  assert.equal(validation.exitCode, 0);
  assert.equal(validation.elevated, mode === 'Elevated');
}
const source = await JSZip.loadAsync(await readFile(join(candidate, 'OneCrew-Lite-0.7.1-win32-x64-candidate.zip')), { checkCRC32: true });
assert.equal(sha(await source.file('onecrew_lite.exe').async('nodebuffer')), receipt.executableSha256);
const zip = new JSZip(), prefix = 'OneCrew-Lite/';
const expected = new Map<string, string>();
function include(name: string, bytes: Buffer) {
  assert(!expected.has(name));
  expected.set(name, sha(bytes));
  zip.file(prefix + name, bytes);
}
for (const entry of Object.values(source.files) as { name: string; dir: boolean; async(kind: string): Promise<Buffer> }[]) {
  if (entry.dir) continue;
  assert(/^(onecrew_lite\.exe|candidate\.json|runtime-manifest\.json|licenses\/node\.txt|licenses\/[a-f0-9]+\/[^/]+)$/.test(entry.name), entry.name);
  include(entry.name, await entry.async('nodebuffer'));
}
for (const name of ['OneCrew.ps1', 'Start-OneCrew.cmd', 'Stop-OneCrew.cmd', 'Upgrade-OneCrew.cmd', 'README.zh-CN.md', 'RELEASE-STATUS.md', 'INSTALL-CODEX.md', 'CODEX-PROMPT.zh-CN.md']) {
  const bytes = await readFile(resolve('development/windows-customer-package', name));
  if (name.endsWith('.ps1')) assert.deepEqual([...bytes.subarray(0, 3)], [239, 187, 191]);
  const content = bytes.toString('utf8');
  if (name.endsWith('.md')) {
    const allowed = new Set([receipt.executableSha256, receipt.payloadSha256]);
    for (const hash of content.match(/\b[a-f0-9]{64}\b/gu) ?? []) assert(allowed.has(hash), `${name}: stale build hash`);
    assert.doesNotMatch(content, /C:[\\/]Users[\\/](?:chris|Administrator)[\\/]/iu, `${name}: customer-specific path`);
  }
  if (name === 'INSTALL-CODEX.md') {
    assert(content.includes(revision));
    assert.equal((content.match(new RegExp(receipt.executableSha256, 'g')) ?? []).length, 2);
    assert(content.includes('SHA256SUMS.json'));
    assert(content.includes('$checksums.PSObject.Properties'));
  }
  if (name === 'RELEASE-STATUS.md') assert(content.includes(revision));
  if (name === 'CODEX-PROMPT.zh-CN.md') for (const required of ['INSTALL-CODEX.md', 'RELEASE-STATUS.md', 'SHA256SUMS.json', 'candidate.json', '首次安装', '旧安装', '原数据目录', 'Codex 内置浏览器']) assert(content.includes(required), `${name}: ${required}`);
  include(name, bytes);
}
include('SHA256SUMS.json', Buffer.from(JSON.stringify(Object.fromEntries(expected), null, 2) + '\n'));
await mkdir(resolve('dist'), { recursive: true });
const bytes = await zip.generateAsync({ type: 'nodebuffer', compression: 'DEFLATE', compressionOptions: { level: 6 }, platform: 'DOS' });
await writeFile(output, bytes, { flag: 'wx' });
const archived = await readFile(output);
const verified = await JSZip.loadAsync(archived, { checkCRC32: true });
assert.equal(Object.values(verified.files).filter((f: any) => !f.dir).length, expected.size);
for (const [name, digest] of expected) assert.equal(sha(await verified.file(prefix + name).async('nodebuffer')), digest, name);
const sums = JSON.parse(await verified.file(prefix + 'SHA256SUMS.json').async('string'));
assert.equal(Object.keys(sums).length + 1, expected.size);
for (const [name, digest] of Object.entries(sums)) assert.equal(sha(await verified.file(prefix + name).async('nodebuffer')), digest, name);
assert.equal(sums['onecrew_lite.exe'], receipt.executableSha256);
await writeFile(output + '.sha256', `${sha(archived)}  ${basename(output)}\r\n`, { flag: 'wx' });
await writeFile(output + '.release.json', JSON.stringify({ product: 'onecrew_lite', version: receipt.productVersion, revision, target: receipt.target, archive: basename(output), bytes: archived.length, sha256: sha(archived), executableSha256: receipt.executableSha256, payloadSha256: receipt.payloadSha256, installationGuide: 'OneCrew-Lite/INSTALL-CODEX.md', productionReady: receipt.productionReady }, null, 2) + '\n', { flag: 'wx' });
console.log(JSON.stringify({ archive: output, sha256: sha(archived), bytes: archived.length, verifiedFiles: expected.size, productionReady: receipt.productionReady }));
