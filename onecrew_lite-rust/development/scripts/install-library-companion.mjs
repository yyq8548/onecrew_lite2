// Install only a manifest-verified Library companion beside the native CLI.
import { createHash, randomUUID } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { cp, lstat, mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { parseArgs } from 'node:util';

const { values } = parseArgs({ options: { 'library-dir': { type: 'string' }, 'install-path': { type: 'string' }, 'check-only': { type: 'boolean', default: false } } });
if (!values['library-dir'] || !values['install-path'] || !path.isAbsolute(values['install-path'])) throw Error('Absolute install path and Library artifact required');
const source = path.resolve(values['library-dir']);
const manifest = JSON.parse(await readFile(path.join(source, 'manifest.json'), 'utf8'));
if (manifest.target !== `${process.platform}-${process.arch}` || !Array.isArray(manifest.files) || manifest.files.length > 1000) throw Error('Library artifact target or manifest invalid');
if (!['discovery-contacts-v2', 'recommendation-capture-v1'].every(capability => manifest.capabilities?.includes(capability))) throw Error('Library companion incompatible; rebuild and install the matching workspace artifact');
const name = process.platform === 'win32' ? 'onecrew_lite.exe' : 'onecrew_lite';
if (!manifest.files.some(file => file.path === name)) throw Error('Library executable missing');
const seen = new Set();
for (const file of manifest.files) {
  if (typeof file.path !== 'string' || !(file.path === name || file.path.startsWith('licenses/'))
    || file.path.includes('\\') || file.path.split('/').some(part => !part || part === '.' || part === '..') || seen.has(file.path)) throw Error('Library artifact path invalid');
  seen.add(file.path);
  if (!Number.isSafeInteger(file.size) || file.size <= 0 || !/^[a-f0-9]{64}$/.test(file.sha256)) throw Error('Library artifact integrity metadata invalid');
  const original = path.join(source, file.path);
  if (!(await lstat(original)).isFile()) throw Error('Library artifact must be a regular file');
  const bytes = await readFile(original);
  if (bytes.length !== file.size || createHash('sha256').update(bytes).digest('hex') !== file.sha256) throw Error('Library artifact integrity mismatch');
}
const destination = path.join(path.dirname(path.resolve(values['install-path'])), 'onecrew_lite_library');
if (values['check-only']) {
  console.log(`Verified compatible Library ${manifest.version} (${manifest.target})`);
  process.exit(0);
}
await mkdir(destination, { recursive: true });
if ((await lstat(destination)).isSymbolicLink()) throw Error('Library destination must not be a symlink');
const binary = path.join(destination, name);
const expected = manifest.files.find(file => file.path === name).sha256;
let existing;
try { existing = createHash('sha256').update(await readFile(binary)).digest('hex'); } catch (error) { if (error.code !== 'ENOENT') throw error; }
if (existing !== expected) {
  if (existing) execFileSync(binary, ['stop'], { stdio: 'ignore', windowsHide: true, timeout: 30000 });
  const staged = path.join(destination, `.onecrew-${randomUUID()}`);
  try {
    await cp(path.join(source, name), staged);
    if (createHash('sha256').update(await readFile(staged)).digest('hex') !== expected) throw Error('Staged Library integrity mismatch');
    await rename(staged, binary);
  }
  finally { await rm(staged, { force: true }); }
}
for (const file of manifest.files.filter(file => file.path !== name)) {
  const target = path.join(destination, file.path);
  await mkdir(path.dirname(target), { recursive: true });
  await cp(path.join(source, file.path), target);
}
await writeFile(path.join(destination, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`);
if (createHash('sha256').update(await readFile(binary)).digest('hex') !== expected) throw Error('Installed Library integrity mismatch');
console.log(`Installed Library ${manifest.version} (${manifest.target}); sha256 ${expected}`);
