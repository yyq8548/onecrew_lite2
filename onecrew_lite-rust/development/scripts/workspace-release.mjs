// Native build and offline release collection. Never publishes or installs.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { lstat, readFile, readdir, writeFile, mkdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { packageWorkspace } from './package-workspace.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
export const targets = {
  'win32-x64': 'x86_64-pc-windows-msvc',
  'darwin-arm64': 'aarch64-apple-darwin',
  'darwin-x64': 'x86_64-apple-darwin',
};
const capabilities = ['discovery-contacts-v2', 'recommendation-capture-v1'];
const json = async file => JSON.parse(await readFile(file, 'utf8'));
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const save = (file, value) => writeFile(file, JSON.stringify(value, null, 2) + '\n', { flag: 'wx' });

export function nativeTarget(platform, arch) {
  const target = targets[`${platform}-${arch}`];
  if (!target) throw Error('Unsupported native release host');
  return target;
}

export function sourceIdentity(cwd = root) {
  const git = args => execFileSync('git', args, { cwd, encoding: 'utf8', windowsHide: true }).trim();
  const repository = git(['rev-parse', '--show-toplevel']);
  if (execFileSync('git', ['status', '--porcelain', '--untracked-files=normal'],
    { cwd: repository, encoding: 'utf8', windowsHide: true }).trim())
    throw Error('Release requires a clean repository, including untracked source files');
  const commit = git(['rev-parse', 'HEAD']);
  if (!/^[a-f0-9]{40}$/.test(commit)) throw Error('Invalid source commit');
  return commit;
}

async function regularFiles(directory, prefix = '') {
  const result = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const relative = prefix + entry.name;
    if (entry.isDirectory()) result.push(...await regularFiles(path.join(directory, entry.name), relative + '/'));
    else if (entry.isFile()) result.push(relative);
    else throw Error('Release contains a symlink or special file');
  }
  return result.sort();
}

export async function verifyBundle(directory) {
  if (!(await lstat(directory)).isDirectory()) throw Error('Bundle must be a real directory');
  const receipt = await json(path.join(directory, 'release-build.json'));
  const manifest = await json(path.join(directory, 'manifest.json'));
  const manifestBytes = await readFile(path.join(directory, 'manifest.json'));
  if (receipt.schema_version !== 1 || receipt.node !== 'v26.7.0' || receipt.plugin !== 'embedded-in-cli' ||
      !/^[a-f0-9]{40}$/.test(receipt.commit) || !/^\d+\.\d+\.\d+$/.test(receipt.version) ||
      receipt.manifest_sha256 !== hash(manifestBytes) || receipt.target !== manifest.target ||
      receipt.version !== manifest.version || targets[receipt.host] !== receipt.target ||
      manifest.product !== 'onecrew_lite-workspace' || manifest.schema_version !== 1)
    throw Error('Invalid release provenance or manifest');
  if (!Array.isArray(manifest.files) || !manifest.files.length) throw Error('Missing bundle files');
  const seen = new Set();
  for (const file of manifest.files) {
    if (typeof file.path !== 'string' || !file.path || file.path.includes('\\') ||
        file.path.includes(':') || file.path.includes('\0') || file.path.startsWith('/') ||
        file.path.split('/').some(part => !part || part === '.' || part === '..') ||
        ['manifest.json', 'release-build.json'].includes(file.path) || seen.has(file.path))
      throw Error('Unsafe or duplicate bundle path');
    seen.add(file.path);
  }
  const actualPaths = await regularFiles(directory);
  const expectedPaths = [...seen, 'manifest.json', 'release-build.json'].sort();
  if (JSON.stringify(actualPaths) !== JSON.stringify(expectedPaths)) throw Error('Unexpected or missing bundle files');
  for (const file of manifest.files) {
    const bytes = await readFile(path.join(directory, file.path));
    if (!Number.isSafeInteger(file.size) || file.size < 1 || bytes.length !== file.size || hash(bytes) !== file.sha256)
      throw Error('Bundle integrity mismatch');
  }
  const library = await json(path.join(directory, 'onecrew_lite_library/manifest.json'));
  const cli = await json(path.join(directory, 'workbench-manifest.json'));
  const binary = receipt.host.startsWith('win32') ? 'onecrew_lite.exe' : 'onecrew_lite';
  const cliFile = manifest.files.find(file => file.path === binary);
  const libraryFile = manifest.files.find(file => file.path === `onecrew_lite_library/${binary}`);
  const libraryBinary = library.files?.find(file => file.path === binary);
  if (!cliFile || !libraryFile || !libraryBinary || cli.sha256 !== cliFile.sha256 ||
      cli.size !== cliFile.size || cli.target !== receipt.target || cli.version !== receipt.version ||
      library.target !== receipt.host || library.version !== manifest.library_version ||
      libraryBinary.sha256 !== libraryFile.sha256 || libraryBinary.size !== libraryFile.size ||
      !capabilities.every(value => library.capabilities?.includes(value)))
    throw Error('Incompatible component pair');
  return { ...receipt, library_version: library.version, capabilities: library.capabilities,
    cli_sha256: cliFile.sha256, library_sha256: libraryFile.sha256 };
}

export async function collectRelease(input, output) {
  const bundles = [];
  for (const host of Object.keys(targets)) {
    const directory = path.join(input, host);
    const receipt = await verifyBundle(directory);
    if (receipt.host !== host) throw Error('Bundle host directory mismatch');
    bundles.push({ ...receipt, directory: host });
  }
  if (new Set(bundles.map(b => b.commit)).size !== 1 || new Set(bundles.map(b => b.version)).size !== 1 ||
      new Set(bundles.map(b => b.library_version)).size !== 1)
    throw Error('Mixed source commits or component versions');
  const manifest = { schema_version: 1, product: 'onecrew_lite-release',
    version: bundles[0].version, commit: bundles[0].commit,
    acceptance: 'pending-native-runtime', artifacts: bundles };
  await save(output, manifest);
  return manifest;
}

export async function selectBundle(input, platform = process.platform, arch = process.arch) {
  nativeTarget(platform, arch);
  const host = `${platform}-${arch}`;
  const index = await json(path.join(input, 'release-manifest.json'));
  if (index.product !== 'onecrew_lite-release' || index.schema_version !== 1 || !Array.isArray(index.artifacts))
    throw Error('Invalid release index');
  const entries = index.artifacts.filter(entry => entry.host === host);
  if (entries.length !== 1 || entries[0].directory !== host) throw Error('Missing or ambiguous host bundle');
  const directory = path.join(input, host);
  const receipt = await verifyBundle(directory);
  if (receipt.commit !== index.commit || receipt.version !== index.version ||
      receipt.manifest_sha256 !== entries[0].manifest_sha256)
    throw Error('Release index does not match bundle');
  return { directory, ...receipt, acceptance: index.acceptance };
}

async function buildNative(output) {
  const target = nativeTarget(process.platform, process.arch);
  if (process.version !== 'v26.7.0') throw Error('Native release requires Node v26.7.0 for both components');
  const rust = execFileSync('rustc', ['-vV'], { cwd: root, encoding: 'utf8', windowsHide: true });
  if (!rust.split(/\r?\n/).includes(`host: ${target}`)) throw Error('Rust host must match native release target');
  if (process.platform === 'darwin') {
    let translated = '';
    try { translated = execFileSync('/usr/sbin/sysctl', ['-in', 'sysctl.proc_translated'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim(); }
    catch (error) { if (error.status !== 1) throw error; }
    if (translated === '1') throw Error('Rosetta builds are not native Intel release evidence');
  }
  const commit = sourceIdentity();
  const version = (await json(path.join(root, 'development/release/version.json'))).cliPluginBase;
  const host = `${process.platform}-${process.arch}`;
  const destination = path.resolve(output, host);
  try { await lstat(destination); throw Error('Release destination already exists'); }
  catch (error) { if (error.code !== 'ENOENT') throw error; }
  const run = (args, cwd = root) => execFileSync('bash', ['-c', 'exec "$@"', '--', ...args],
    { cwd, stdio: 'inherit', windowsHide: true });
  run(['bash', 'development/scripts/build.sh', '--target', target, '--install-deps']);
  const library = path.resolve(root, '../onecrew_lite-library');
  run(['pnpm', 'install', '--frozen-lockfile'], library);
  run(['pnpm', 'build'], library);
  execFileSync(process.execPath, ['scripts/build-binary.mjs'], { cwd: library, stdio: 'inherit', windowsHide: true });
  if (sourceIdentity() !== commit) throw Error('Source changed during release build');
  await mkdir(path.resolve(output), { recursive: true });
  await packageWorkspace(path.join(root, 'dist/onecrew_lite', version, target),
    path.join(library, 'release', host), destination);
  if (sourceIdentity() !== commit) throw Error('Source changed during packaging');
  await save(path.join(destination, 'release-build.json'), { schema_version: 1,
    commit, version, host, target, node: process.version,
    plugin: 'embedded-in-cli', manifest_sha256: hash(await readFile(path.join(destination, 'manifest.json'))) });
  await verifyBundle(destination);
  console.log(destination);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { positionals, values } = parseArgs({ allowPositionals: true, options: {
    input: { type: 'string' }, output: { type: 'string' },
  } });
  const [command] = positionals;
  if (positionals.length !== 1) throw Error('Choose build, collect, verify or select');
  if (command === 'build' && values.output) await buildNative(values.output);
  else if (command === 'collect' && values.input && values.output) console.log(await collectRelease(values.input, values.output));
  else if (command === 'verify' && values.input) console.log(await verifyBundle(values.input));
  else if (command === 'select' && values.input) console.log(await selectBundle(values.input));
  else throw Error('Usage: workspace-release.mjs build --output DIR | collect --input DIR --output FILE | verify/select --input DIR');
}
