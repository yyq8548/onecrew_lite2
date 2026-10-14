import { build } from 'esbuild';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { chmod, cp, mkdir, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const { version } = JSON.parse(readFileSync(path.join(root, 'package.json'), 'utf8'));
const supported = {
  'darwin-arm64': { platform: 'darwin', arch: 'arm64', artifactName: 'macos-arm64' },
  'darwin-x64': { platform: 'darwin', arch: 'x64', artifactName: 'macos-x64' },
  'linux-arm64': { platform: 'linux', arch: 'arm64', artifactName: 'linux-arm64' },
  'linux-x64': { platform: 'linux', arch: 'x64', artifactName: 'linux-x64' },
  'win32-arm64': { platform: 'win32', arch: 'arm64', artifactName: 'windows-arm64' },
  'win32-x64': { platform: 'win32', arch: 'x64', artifactName: 'windows-x64' },
};
const hostTarget = `${process.platform}-${process.arch}`;
const usage = `Usage: node scripts/build-binary.mjs [--target <target>] [--node <absolute-path>] [--help]

Targets: ${Object.keys(supported).join(', ')}
Defaults to the current OS and architecture. Foreign targets require --node
pointing to an official target Node executable of the same version as the build host
(${process.version}). Use build:release to download matched runtimes automatically.`;
const { values, positionals } = parseArgs({
  args: process.argv.slice(2),
  options: {
    target: { type: 'string' },
    node: { type: 'string' },
    help: { type: 'boolean', short: 'h' },
  },
  allowPositionals: true,
});
if (values.help) {
  console.log(usage);
  process.exit(0);
}
if (positionals.length) throw new Error(`Unexpected argument: ${positionals[0]}\n\n${usage}`);

const target = values.target || hostTarget;
const targetInfo = supported[target];
if (!targetInfo) throw new Error(`Unsupported target ${target}.\n\n${usage}`);
const isHostTarget = target === hostTarget;
if (!isHostTarget && !values.node)
  throw new Error(
    `Cross-target ${target} requires --node with the matching official Node ${process.version} executable.`,
  );
if (values.node && !path.isAbsolute(values.node))
  throw new Error('--node must be an absolute path to the target Node executable.');
const targetNode = path.resolve(values.node || process.execPath);
try {
  const targetNodeStat = await stat(targetNode);
  if (!targetNodeStat.isFile()) throw new Error('not a file');
} catch {
  throw new Error(`Target Node executable does not exist or is not a file: ${targetNode}`);
}

const cacheDir = path.join(root, '.cache', 'sea', target);
const stageDir = path.join(cacheDir, 'stage');
const releaseDir = path.join(root, 'release');
const stageEntry = path.join(stageDir, 'dist', 'cli.js');
const launcher = path.join(root, 'scripts', 'sea-launcher.cjs');
const metafilePath = path.join(cacheDir, 'esbuild-meta.json');

const [nodeMajor, nodeMinor] = process.versions.node.split('.').map(Number);
if (nodeMajor < 25 || (nodeMajor === 25 && nodeMinor < 5)) {
  throw new Error(`Node 25.5 or newer is required for --build-sea (found ${process.version})`);
}
if (target === 'darwin-x64' && process.version !== 'v26.7.0') {
  throw new Error(
    'Intel macOS packaging requires the verified Node v26.7.0 SEA runtime. Use pnpm build:release to download it automatically.',
  );
}

const requiredPaths = [path.join(root, 'dist', 'cli.js'), path.join(root, 'web', 'dist')];
for (const required of requiredPaths) {
  try {
    await stat(required);
  } catch {
    throw new Error(
      `Missing build input ${path.relative(root, required)}; run the normal build first`,
    );
  }
}

const requireFromHere = createRequire(import.meta.url);
const stagedPackageLicenses = [];
const copyTree = async (source, destination) => {
  await mkdir(path.dirname(destination), { recursive: true });
  await cp(source, destination, { recursive: true, dereference: true, force: true });
};

const packageRootFor = (entry, packageName) => {
  let current = path.dirname(entry);
  while (current !== path.dirname(current)) {
    try {
      const metadata = JSON.parse(readFileSync(path.join(current, 'package.json'), 'utf8'));
      if (metadata.name === packageName) return { root: current, metadata };
    } catch {
      // Continue walking through package subdirectories.
    }
    current = path.dirname(current);
  }
  throw new Error(`Could not locate package root for ${packageName} (${entry})`);
};

const stagedPackages = new Set();
const stagePackage = async (packageName, fromEntry = import.meta.url) => {
  const resolver = createRequire(fromEntry);
  const entry = resolver.resolve(packageName);
  const { root: sourceRoot, metadata } = packageRootFor(entry, packageName);
  const destinationRoot = path.join(stageDir, 'node_modules', ...packageName.split('/'));
  if (stagedPackages.has(packageName)) return;
  stagedPackages.add(packageName);
  await copyTree(sourceRoot, destinationRoot);
  stagedPackageLicenses.push({ packageName, sourceRoot });
  for (const dependency of Object.keys(metadata.dependencies || {})) {
    await stagePackage(dependency, entry);
  }
};

await rm(stageDir, { recursive: true, force: true });
await mkdir(path.dirname(stageEntry), { recursive: true });
await mkdir(path.join(stageDir, 'web'), { recursive: true });

const esbuildResult = await build({
  entryPoints: [path.join(root, 'dist', 'cli.js')],
  outfile: stageEntry,
  bundle: true,
  format: 'esm',
  platform: 'node',
  target: 'node25',
  sourcemap: false,
  external: ['@electric-sql/pglite', '@electric-sql/pglite/*', 'fs-native-extensions'],
  plugins: [
    {
      name: 'external-native-addons',
      setup(context) {
        context.onResolve({ filter: /\.node$/ }, (args) => ({ path: args.path, external: true }));
      },
    },
  ],
  banner: {
    js: "import { createRequire as __seaCreateRequire } from 'node:module'; import { fileURLToPath as __seaFileURLToPath } from 'node:url'; globalThis.require = __seaCreateRequire(import.meta.url); globalThis.__filename = __seaFileURLToPath(import.meta.url); globalThis.__dirname = __seaFileURLToPath(new URL('.', import.meta.url));",
  },
  metafile: true,
  tsconfig: path.join(root, 'tsconfig.build.json'),
  absWorkingDir: root,
  logLevel: 'info',
});
await writeFile(metafilePath, JSON.stringify(esbuildResult.metafile));

await copyTree(path.join(root, 'web', 'dist'), path.join(stageDir, 'web', 'dist'));
await writeFile(
  path.join(stageDir, 'runtime-loader.cjs'),
  "module.exports = import('./dist/cli.js');\n",
);
await writeFile(
  path.join(stageDir, 'package.json'),
  `${JSON.stringify({ private: true, type: 'module', version })}\n`,
);

const pgliteEntry = requireFromHere.resolve('@electric-sql/pglite');
const pgliteRoot = path.resolve(path.dirname(pgliteEntry), '..');
await copyTree(
  path.join(pgliteRoot, 'package.json'),
  path.join(stageDir, 'node_modules', '@electric-sql', 'pglite', 'package.json'),
);
await copyTree(
  path.join(pgliteRoot, 'dist'),
  path.join(stageDir, 'node_modules', '@electric-sql', 'pglite', 'dist'),
);

await stagePackage('fs-native-extensions');
const fsNativePrebuilds = path.join(stageDir, 'node_modules', 'fs-native-extensions', 'prebuilds');
const targetPrebuild = path.join(fsNativePrebuilds, target);
const targetNativeAddon = path.join(targetPrebuild, 'fs-native-extensions.node');
try {
  const addonStat = await stat(targetNativeAddon);
  if (!addonStat.isFile()) throw new Error('not a file');
} catch {
  throw new Error(`Missing fs-native-extensions prebuild for ${target}: ${targetNativeAddon}`);
}
for (const entry of await readdir(fsNativePrebuilds)) {
  if (entry !== target)
    await rm(path.join(fsNativePrebuilds, entry), { recursive: true, force: true });
}

const licenseDir = path.join(stageDir, 'licenses');
await mkdir(licenseDir, { recursive: true });
const pLimitEntry = requireFromHere.resolve('p-limit');
const licenseCopies = [
  [
    path.join(packageRootFor(pLimitEntry, 'p-limit').root, 'license'),
    path.join(licenseDir, 'p-limit.LICENSE'),
  ],
  [
    path.join(
      packageRootFor(createRequire(pLimitEntry).resolve('yocto-queue'), 'yocto-queue').root,
      'license',
    ),
    path.join(licenseDir, 'yocto-queue.LICENSE'),
  ],
  [
    path.join(
      packageRootFor(requireFromHere.resolve('kysely-pglite-dialect'), 'kysely-pglite-dialect')
        .root,
      'LICENSE',
    ),
    path.join(licenseDir, 'kysely-pglite-dialect.LICENSE'),
  ],
  [
    path.join(path.dirname(requireFromHere.resolve('xlsx')), 'LICENSE'),
    path.join(licenseDir, 'sheetjs-xlsx.LICENSE'),
  ],
  [path.join(pgliteRoot, 'LICENSE'), path.join(licenseDir, 'electric-sql-pglite.LICENSE')],
  [
    path.join(root, 'vendor', 'teable', 'LICENSE'),
    path.join(licenseDir, 'teable-monorepo.LICENSE'),
  ],
  [
    path.join(root, 'vendor', 'teable', 'packages', 'core', 'LICENSE'),
    path.join(licenseDir, 'teable-packages-MIT.LICENSE'),
  ],
];
for (const { packageName, sourceRoot } of stagedPackageLicenses) {
  licenseCopies.push([
    path.join(sourceRoot, 'LICENSE'),
    path.join(licenseDir, `${packageName.replaceAll('/', '-')}.LICENSE`),
  ]);
}
for (const [source, destination] of licenseCopies) {
  try {
    await copyTree(source, destination);
  } catch {
    // Some platform packages omit a separate license file; the parent package license remains staged.
  }
}

const teablePackages = new Map();
const esbuildMeta = JSON.parse(await readFile(metafilePath, 'utf8'));
for (const input of Object.keys(esbuildMeta.inputs || {})) {
  const absoluteInput = path.resolve(root, input);
  if (!absoluteInput.includes(`${path.sep}vendor${path.sep}teable${path.sep}`)) continue;
  let current = path.dirname(absoluteInput);
  while (current.startsWith(path.join(root, 'vendor', 'teable'))) {
    try {
      const packageJson = JSON.parse(readFileSync(path.join(current, 'package.json'), 'utf8'));
      if (typeof packageJson.name === 'string' && packageJson.name.startsWith('@teable/')) {
        teablePackages.set(packageJson.name, {
          name: packageJson.name,
          version: packageJson.version || null,
          license: packageJson.license || null,
          root: current,
        });
        break;
      }
    } catch {
      // Walk to the package root for this compiled input.
    }
    current = path.dirname(current);
  }
}
await writeFile(
  path.join(licenseDir, 'teable-packages.json'),
  `${JSON.stringify(
    [...teablePackages.values()]
      .sort((a, b) => a.name.localeCompare(b.name))
      .map(({ name, version, license }) => ({
        name,
        version,
        license,
        licenseFile: license === 'MIT' ? 'teable-packages-MIT.LICENSE' : 'teable-monorepo.LICENSE',
      })),
    null,
    2,
  )}\n`,
);

async function filesUnder(directory, relative = '') {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const name = path.join(relative, entry.name);
    const absolute = path.join(directory, entry.name);
    if (entry.isSymbolicLink())
      throw new Error(`Source symlink is not allowed in SEA staging: ${name}`);
    if (entry.isDirectory()) files.push(...(await filesUnder(absolute, name)));
    else if (entry.isFile() && !name.endsWith('.map'))
      files.push({ absolute, relative: name.split(path.sep).join('/') });
    else if (!entry.isFile()) throw new Error(`Unsupported staging entry: ${name}`);
  }
  return files;
}

const stagedFiles = (await filesUnder(stageDir)).sort((a, b) =>
  a.relative.localeCompare(b.relative),
);
const manifestFiles = [];
for (const file of stagedFiles) {
  const data = await readFile(file.absolute);
  manifestFiles.push({
    asset: file.relative,
    path: file.relative,
    size: data.byteLength,
    hash: createHash('sha256').update(data).digest('hex'),
  });
}
const manifestBase = { version: 1, files: manifestFiles };
const payloadHash = createHash('sha256').update(JSON.stringify(manifestBase)).digest('hex');
const manifest = { ...manifestBase, payloadHash };
const manifestPath = path.join(cacheDir, 'runtime-manifest.json');
await mkdir(cacheDir, { recursive: true });
await writeFile(manifestPath, `${JSON.stringify(manifest)}\n`);

const seaConfigPath = path.join(cacheDir, 'sea-config.json');
const assets = { [path.relative(root, manifestPath).split(path.sep).join('/')]: manifestPath };
for (const file of manifestFiles)
  assets[file.asset] = path.join(stageDir, ...file.asset.split('/'));
const seaConfig = {
  main: path.relative(root, launcher),
  executable: targetNode,
  output: path.join(
    releaseDir,
    `onecrew_lite-${targetInfo.artifactName}${targetInfo.platform === 'win32' ? '.exe' : ''}`,
  ),
  disableExperimentalSEAWarning: true,
  useSnapshot: false,
  useCodeCache: false,
  assets,
};
// Keep the manifest asset key independent of its workspace path in the config.
delete assets[path.relative(root, manifestPath).split(path.sep).join('/')];
assets['runtime-manifest.json'] = manifestPath;
await writeFile(seaConfigPath, `${JSON.stringify(seaConfig, null, 2)}\n`);
await mkdir(releaseDir, { recursive: true });
const output = seaConfig.output;
await rm(output, { force: true });
if (target === 'darwin-x64') {
  // Node's built-in Mach-O injector corrupts Intel static initializers:
  // https://github.com/nodejs/node/issues/65479. Use the documented SEA blob flow.
  const { inject } = await import('postject');
  const blobPath = path.join(cacheDir, 'sea.blob');
  const blobConfigPath = path.join(cacheDir, 'sea-blob-config.json');
  const { executable: _executable, ...blobConfig } = seaConfig;
  await writeFile(blobConfigPath, JSON.stringify({ ...blobConfig, output: blobPath }));
  execFileSync(process.execPath, [`--experimental-sea-config=${blobConfigPath}`], {
    cwd: root,
    stdio: 'inherit',
  });
  await cp(targetNode, output);
  if (process.platform === 'darwin')
    execFileSync('/usr/bin/codesign', ['--remove-signature', output], { stdio: 'inherit' });
  await inject(output, 'NODE_SEA_BLOB', await readFile(blobPath), {
    sentinelFuse: 'NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2',
    machoSegmentName: 'NODE_SEA',
  });
} else {
  execFileSync(process.execPath, [`--build-sea=${seaConfigPath}`], { cwd: root, stdio: 'inherit' });
}
await chmod(output, 0o755);
if (targetInfo.platform === 'darwin' && process.platform === 'darwin') {
  execFileSync('/usr/bin/codesign', ['--force', '--sign', '-', output], { stdio: 'inherit' });
}
const outputStat = await stat(output);
console.log(`Built ${path.relative(root, output)} (${outputStat.size} bytes)`);
console.log(`SEA payload ${payloadHash} (${manifestFiles.length} files)`);

// The native workbench consumes the companion through the Workspace manifest.
const companionDir = path.join(releaseDir, target);
// Rebuild generated payload from scratch; never carry binaries from another edition.
await rm(companionDir, { recursive: true, force: true });
await mkdir(companionDir, { recursive: true });
const companionName = targetInfo.platform === 'win32' ? 'onecrew_lite.exe' : 'onecrew_lite';
await cp(output, path.join(companionDir, companionName));
await cp(licenseDir, path.join(companionDir, 'licenses'), { recursive: true });
const companionFiles = [];
for (const { absolute, relative } of await filesUnder(companionDir)) {
  if (relative === 'manifest.json') continue;
  const data = await readFile(absolute);
  companionFiles.push({ path: relative, size: data.length, sha256: createHash('sha256').update(data).digest('hex') });
}
await writeFile(path.join(companionDir, 'manifest.json'), `${JSON.stringify({ version, target, capabilities: ['discovery-contacts-v2', 'recommendation-capture-v1'], files: companionFiles }, null, 2)}\n`);
