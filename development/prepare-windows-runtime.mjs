import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { parseArgs } from 'node:util';

const { values } = parseArgs({ options: { output: { type: 'string' }, 'seven-zip': { type: 'string' }, redist: { type: 'string' } } });
assert.ok(values.output && values['seven-zip'], 'Usage: node development/prepare-windows-runtime.mjs --output <new directory> --seven-zip <7zz path> [--redist <verified download>]');
const source = JSON.parse(await readFile(new URL('./windows-runtime.json', import.meta.url), 'utf8'));
const output = path.resolve(values.output);
await mkdir(path.dirname(output), { recursive: true });
await mkdir(output);
let bytes;
if (values.redist) bytes = await readFile(path.resolve(values.redist));
else {
  const response = await fetch(source.url, { signal: AbortSignal.timeout(120_000) });
  assert.equal(response.status, 200, 'Microsoft runtime download failed');
  bytes = Buffer.from(await response.arrayBuffer());
}
const sha256 = value => createHash('sha256').update(value).digest('hex');
assert.equal(sha256(bytes), source.sha256, 'Microsoft redistributable SHA-256');
const archive = path.join(output, 'VC_redist.x64.exe');
await writeFile(archive, bytes, { flag: 'wx' });
const extractor = path.resolve(values['seven-zip']);
const extract = args => execFileSync(extractor, args, { stdio: ['ignore', 'pipe', 'pipe'] });
const cabs = path.join(output, 'cabs'), extracted = path.join(output, 'extracted');
extract(['e', '-t#', archive, '4.cab', '-o' + cabs, '-y']);
extract(['e', path.join(cabs, '4.cab'), 'a4', '-o' + cabs, '-y']);
extract(['e', path.join(cabs, 'a4'), source.member, '-o' + extracted, '-y']);
const dll = path.join(extracted, source.member);
assert.equal(sha256(await readFile(dll)), source.dllSha256, 'Microsoft runtime DLL SHA-256');
await copyFile(dll, path.join(output, 'vcruntime140.dll'));
await writeFile(path.join(output, 'source.json'), JSON.stringify(source, null, 2) + '\n', { flag: 'wx' });
console.log(JSON.stringify({ directory: output, version: source.version, dllSha256: source.dllSha256 }));
