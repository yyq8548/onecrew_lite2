import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { chmod, copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { parsePayloadManifest } from '../onecrew_lite-app/src/packaging/payload-manifest.ts';

const root = fileURLToPath(new URL('../', import.meta.url));
const require = createRequire(new URL('../onecrew_lite-app/package.json', import.meta.url));
const JSZip = require('jszip');
const semver = require('semver');
const { values } = parseArgs({ options: { catalog: { type: 'string' }, 'mac-zip': { type: 'string' }, 'windows-candidate': { type: 'string' }, output: { type: 'string' } } });
assert.ok(values.output, 'Usage: node development/build-npm.mjs --catalog <verified release inputs> [--mac-zip <verified ZIP>] [--windows-candidate <candidate directory>] --output <new directory>');
const catalog = JSON.parse(await readFile(values.catalog ? path.resolve(values.catalog) : new URL('./npm-release-inputs.json', import.meta.url), 'utf8'));
const workspace = JSON.parse(await readFile(path.join(root, 'package.json'), 'utf8'));
assert.ok(typeof catalog.version === 'string' && semver.valid(catalog.version) === catalog.version, 'Main package version must be an exact semantic version');
assert.equal(catalog.version, workspace.version);
const output = path.resolve(values.output);
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const inputs = [];
const platformInputs = {};
const optionalDependencies = {};

for (const [target, metadata] of Object.entries(catalog.platforms)) {
  assert.ok(['darwin-arm64', 'win32-x64'].includes(target), 'Unsupported npm platform');
  const [platform, arch] = target.split('-');
  assert.equal(metadata.platform, platform);
  assert.equal(metadata.arch, arch);
  assert.equal(metadata.executable, platform === 'win32' ? 'onecrew_lite.exe' : 'onecrew_lite');
  const version = metadata.version === undefined ? catalog.version : metadata.version;
  assert.ok(typeof version === 'string' && semver.valid(version) === version, `${target} package version must be an exact semantic version`);
  platformInputs[target] = { ...metadata, version };
  optionalDependencies['@onecrew/lite-' + target] = version;
  if (metadata.reusePublished === true) {
    const published = JSON.parse(execFileSync('npm', ['view', '@onecrew/lite-' + target + '@' + version, 'version', '--json'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] }));
    assert.equal(published, version, `${target} reused package must already exist`);
    continue;
  }
  let candidate, runtimeManifest, included;
  if (target === 'win32-x64') {
    assert.ok(values['windows-candidate'], 'Windows npm requires --windows-candidate');
    const directory = path.resolve(values['windows-candidate']);
    const receiptBytes = await readFile(path.join(directory, 'candidate.json'));
    const manifestBytes = await readFile(path.join(directory, 'runtime-manifest.json'));
    candidate = JSON.parse(receiptBytes);
    runtimeManifest = parsePayloadManifest(manifestBytes);
    assert.equal(digest(manifestBytes), metadata.runtimeManifestSha256, 'Windows runtime manifest SHA-256');
    assert.equal(candidate.payloadSha256, runtimeManifest.payloadSha256);
    const executable = await readFile(path.join(directory, metadata.executable));
    assert.equal(executable.length, metadata.executableBytes, 'Windows executable size');
    assert.equal(digest(executable), metadata.executableSha256, 'Windows executable SHA-256');
    const microsoft = JSON.parse(await readFile(new URL('./windows-runtime.json', import.meta.url), 'utf8'));
    const dll = runtimeManifest.files.find(file => file.path === 'runtime/vcruntime140.dll');
    assert.equal(dll?.sha256, microsoft.dllSha256, 'Windows HTTP runtime must be included');
    assert.equal(digest(await readFile(path.join(directory, 'payload/runtime/vcruntime140.dll'))), microsoft.dllSha256);
    included = [{ path: metadata.executable, data: executable }, { path: 'candidate.json', data: receiptBytes }, { path: 'runtime-manifest.json', data: manifestBytes }];
    for (const file of runtimeManifest.files.filter(file => file.path.startsWith('licenses/'))) {
      const data = await readFile(path.join(directory, 'payload', ...file.path.split('/')));
      assert.equal(data.length, file.size, file.path);
      assert.equal(digest(data), file.sha256, file.path);
      included.push({ path: file.path, data });
    }
  } else {
    assert.ok(values['mac-zip'], 'Mac npm requires --mac-zip unless reusePublished is true');
    const archive = await readFile(path.resolve(values['mac-zip']));
    assert.equal(archive.length, metadata.size, `${target} ZIP size`);
    assert.equal(digest(archive), metadata.sha256, `${target} ZIP SHA-256`);
    const zip = await JSZip.loadAsync(archive, { checkCRC32: true }), prefix = 'OneCrew-Lite/';
    const sums = JSON.parse(await zip.file(prefix + 'SHA256SUMS.json').async('string'));
    const files = Object.values(zip.files).filter(entry => !entry.dir);
    assert.equal(files.length, Object.keys(sums).length + 1, `${target} checksum coverage`);
    for (const [file, sha256] of Object.entries(sums)) {
      assert.match(sha256, /^[a-f0-9]{64}$/);
      assert.equal(digest(await zip.file(prefix + file).async('nodebuffer')), sha256, file);
    }
    candidate = JSON.parse(await zip.file(prefix + 'candidate.json').async('string'));
    runtimeManifest = JSON.parse(await zip.file(prefix + 'runtime-manifest.json').async('string'));
    included = [];
    for (const entry of files.filter(entry => {
      const file = entry.name.slice(prefix.length);
      return [metadata.executable, 'candidate.json', 'runtime-manifest.json'].includes(file) || file.startsWith('licenses/');
    })) included.push({ path: entry.name.slice(prefix.length), data: await entry.async('nodebuffer') });
    assert.equal(digest(included.find(file => file.path === metadata.executable).data), metadata.executableSha256);
  }
  assert.equal(candidate.productVersion, version);
  assert.equal(candidate.target, target);
  assert.equal(runtimeManifest.productVersion, version);
  assert.equal(runtimeManifest.target, target);
  assert.equal(runtimeManifest.source.checkoutCommit, metadata.buildCheckout, `${target} source checkout`);
  assert.equal(runtimeManifest.source.dirty, metadata.sourceDirty, `${target} source state`);
  assert.equal(candidate.executableSha256, metadata.executableSha256);
  assert.ok(included.some(entry => entry.path === 'licenses/node.txt'));
  inputs.push({ target, version, metadata, included });
}

await mkdir(path.dirname(output), { recursive: true });
await mkdir(output);
const common = { version: catalog.version, license: 'UNLICENSED', homepage: 'https://onecrewai.io', publishConfig: { access: 'public', registry: 'https://registry.npmjs.org/' } };
const packages = [];
const writeJson = (file, value) => writeFile(file, JSON.stringify(value, null, 2) + '\n');
async function pack(directory, name, version, requiredFiles) {
  const [result] = JSON.parse(execFileSync('npm', ['pack', '--json', '--ignore-scripts', '--pack-destination', output], { cwd: directory, encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] }));
  assert.equal(result.name, name);
  assert.equal(result.version, version);
  assert.deepEqual(result.files.map(file => file.path).sort(), [...requiredFiles, 'package.json', 'README.md'].sort(), `${name} packed files`);
  const { filename, size, unpackedSize, integrity, shasum } = result;
  packages.push({ name, ...common, version, filename, size, unpackedSize, integrity, shasum });
  console.log(`Packed ${name}@${version}: ${filename} (${size} bytes)`);
}

for (const input of inputs) {
  const name = '@onecrew/lite-' + input.target;
  const directory = path.join(output, input.target);
  await mkdir(directory);
  for (const entry of input.included) {
    const file = entry.path;
    const destination = path.resolve(directory, file);
    assert.ok(destination.startsWith(directory + path.sep));
    await mkdir(path.dirname(destination), { recursive: true });
    await writeFile(destination, entry.data);
  }
  if (input.metadata.platform !== 'win32') await chmod(path.join(directory, input.metadata.executable), 0o755);
  await writeJson(path.join(directory, 'package.json'), { name, ...common, version: input.version, description: `OneCrew Lite native application for ${input.target}`, os: [input.metadata.platform], cpu: [input.metadata.arch], files: [input.metadata.executable, 'licenses/', 'candidate.json', 'runtime-manifest.json'] });
  await writeFile(path.join(directory, 'README.md'), `# ${name}\n\nNative OneCrew Lite ${input.version}. Install @onecrew/lite to select the matching platform. The application and third-party notices are included; installation does not run scripts or download another program.\n`);
  await pack(directory, name, input.version, input.included.map(entry => entry.path));
}

const main = path.join(output, 'lite');
await mkdir(path.join(main, 'bin'), { recursive: true });
await copyFile(new URL('./npm-entry.cjs', import.meta.url), path.join(main, 'bin/onecrew-lite.cjs'));
await chmod(path.join(main, 'bin/onecrew-lite.cjs'), 0o755);
await writeJson(path.join(main, 'package.json'), { name: '@onecrew/lite', ...common, description: 'OneCrew Lite local creator research workbench and MCP bridge', keywords: ['onecrew', 'lite', 'mcp', 'creator'], engines: { node: '>=20' }, os: ['darwin', 'win32'], cpu: ['arm64', 'x64'], bin: { 'onecrew-lite': 'bin/onecrew-lite.cjs' }, files: ['bin/'], optionalDependencies });
await writeFile(path.join(main, 'README.md'), (await readFile(new URL('./npm-README.md', import.meta.url), 'utf8')).replaceAll('__VERSION__', catalog.version).replaceAll('__MAC_VERSION__', optionalDependencies['@onecrew/lite-darwin-arm64']).replaceAll('__WINDOWS_VERSION__', optionalDependencies['@onecrew/lite-win32-x64']));
await pack(main, '@onecrew/lite', catalog.version, ['bin/onecrew-lite.cjs']);
await writeJson(path.join(output, 'release.json'), { version: catalog.version, sourceCommit: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(), sourceDirty: execFileSync('git', ['status', '--porcelain'], { cwd: root, encoding: 'utf8' }).trim().length > 0, inputs: platformInputs, packages });
console.log('No packages published. Publish the platform tarballs first, then the main tarball; verify before assigning latest.');
