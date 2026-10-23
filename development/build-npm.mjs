import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { chmod, copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const root = fileURLToPath(new URL('../', import.meta.url));
const require = createRequire(new URL('../onecrew_lite-app/package.json', import.meta.url));
const JSZip = require('jszip');
const catalog = JSON.parse(await readFile(new URL('./npm-release-inputs.json', import.meta.url), 'utf8'));
const workspace = JSON.parse(await readFile(path.join(root, 'package.json'), 'utf8'));
assert.equal(catalog.version, workspace.version);
const { values } = parseArgs({ options: { 'mac-zip': { type: 'string' }, 'windows-zip': { type: 'string' }, output: { type: 'string' } } });
assert.ok(values['mac-zip'] && values['windows-zip'] && values.output, 'Usage: node development/build-npm.mjs --mac-zip <verified ZIP> --windows-zip <verified ZIP> --output <new directory>');
const output = path.resolve(values.output);
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const inputs = [];

for (const [target, metadata] of Object.entries(catalog.platforms)) {
  const archive = await readFile(path.resolve(values[target === 'darwin-arm64' ? 'mac-zip' : 'windows-zip']));
  assert.equal(archive.length, metadata.size, `${target} ZIP size`);
  assert.equal(digest(archive), metadata.sha256, `${target} ZIP SHA-256`);
  const zip = await JSZip.loadAsync(archive, { checkCRC32: true });
  const prefix = 'OneCrew-Lite/';
  const sums = JSON.parse(await zip.file(prefix + 'SHA256SUMS.json').async('string'));
  const files = Object.values(zip.files).filter(entry => !entry.dir);
  assert.equal(files.length, Object.keys(sums).length + 1, `${target} checksum coverage`);
  for (const [file, sha256] of Object.entries(sums)) {
    assert.match(sha256, /^[a-f0-9]{64}$/);
    assert.equal(digest(await zip.file(prefix + file).async('nodebuffer')), sha256, file);
  }
  const candidate = JSON.parse(await zip.file(prefix + 'candidate.json').async('string'));
  assert.equal(candidate.productVersion, catalog.version);
  assert.equal(candidate.target, target);
  assert.equal(candidate.executableSha256, metadata.executableSha256);
  assert.equal(digest(await zip.file(prefix + metadata.executable).async('nodebuffer')), metadata.executableSha256);
  const included = files.filter(entry => {
    const file = entry.name.slice(prefix.length);
    return [metadata.executable, 'candidate.json', 'runtime-manifest.json'].includes(file) || file.startsWith('licenses/');
  });
  assert.ok(included.some(entry => entry.name === prefix + 'licenses/node.txt'));
  inputs.push({ target, metadata, zip, prefix, included });
}

await mkdir(path.dirname(output), { recursive: true });
await mkdir(output);
const common = { version: catalog.version, license: 'UNLICENSED', homepage: 'https://onecrewai.io', publishConfig: { access: 'public', registry: 'https://registry.npmjs.org/' } };
const optionalDependencies = {};
const packages = [];
const writeJson = (file, value) => writeFile(file, JSON.stringify(value, null, 2) + '\n');
async function pack(directory, name, requiredFiles) {
  const [result] = JSON.parse(execFileSync('npm', ['pack', '--json', '--ignore-scripts', '--pack-destination', output], { cwd: directory, encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] }));
  assert.equal(result.name, name);
  assert.equal(result.version, catalog.version);
  assert.deepEqual(result.files.map(file => file.path).sort(), [...requiredFiles, 'package.json', 'README.md'].sort(), `${name} packed files`);
  const { filename, size, unpackedSize, integrity, shasum } = result;
  packages.push({ name, ...common, filename, size, unpackedSize, integrity, shasum });
  console.log(`Packed ${name}@${catalog.version}: ${filename} (${size} bytes)`);
}

for (const input of inputs) {
  const name = '@onecrew/lite-' + input.target;
  const directory = path.join(output, input.target);
  await mkdir(directory);
  for (const entry of input.included) {
    const file = entry.name.slice(input.prefix.length);
    const destination = path.resolve(directory, file);
    assert.ok(destination.startsWith(directory + path.sep));
    await mkdir(path.dirname(destination), { recursive: true });
    await writeFile(destination, await entry.async('nodebuffer'));
  }
  if (input.metadata.platform !== 'win32') await chmod(path.join(directory, input.metadata.executable), 0o755);
  await writeJson(path.join(directory, 'package.json'), { name, ...common, description: `OneCrew Lite native application for ${input.target}`, os: [input.metadata.platform], cpu: [input.metadata.arch], files: [input.metadata.executable, 'licenses/', 'candidate.json', 'runtime-manifest.json'] });
  await writeFile(path.join(directory, 'README.md'), `# ${name}\n\nNative OneCrew Lite ${catalog.version}. Install @onecrew/lite to select the matching platform. The application and third-party notices are included; installation does not run scripts or download another program.\n`);
  await pack(directory, name, input.included.map(entry => entry.name.slice(input.prefix.length)));
  optionalDependencies[name] = catalog.version;
}

const main = path.join(output, 'lite');
await mkdir(path.join(main, 'bin'), { recursive: true });
await copyFile(new URL('./npm-entry.cjs', import.meta.url), path.join(main, 'bin/onecrew-lite.cjs'));
await chmod(path.join(main, 'bin/onecrew-lite.cjs'), 0o755);
await writeJson(path.join(main, 'package.json'), { name: '@onecrew/lite', ...common, description: 'OneCrew Lite local creator research workbench and MCP bridge', keywords: ['onecrew', 'lite', 'mcp', 'creator'], engines: { node: '>=20' }, os: ['darwin', 'win32'], cpu: ['arm64', 'x64'], bin: { 'onecrew-lite': 'bin/onecrew-lite.cjs' }, files: ['bin/'], optionalDependencies });
await writeFile(path.join(main, 'README.md'), (await readFile(new URL('./npm-README.md', import.meta.url), 'utf8')).replaceAll('__VERSION__', catalog.version));
await pack(main, '@onecrew/lite', ['bin/onecrew-lite.cjs']);
await writeJson(path.join(output, 'release.json'), { version: catalog.version, sourceCommit: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(), sourceDirty: execFileSync('git', ['status', '--porcelain'], { cwd: root, encoding: 'utf8' }).trim().length > 0, inputs: catalog.platforms, packages });
console.log('No packages published. Publish the platform tarballs first, then the main tarball; verify before assigning latest.');
