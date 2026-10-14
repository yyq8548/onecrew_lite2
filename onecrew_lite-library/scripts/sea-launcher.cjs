'use strict';

const { getAsset, isSea } = require('node:sea');
const {
  access,
  chmod,
  mkdir,
  mkdtemp,
  readFile,
  rename,
  rm,
  writeFile,
} = require('node:fs/promises');
const { constants } = require('node:fs');
const { createHash } = require('node:crypto');
const { createRequire } = require('node:module');
const os = require('node:os');
const path = require('node:path');

const MANIFEST_ASSET = 'runtime-manifest.json';
const COMPLETE_MARKER = '.complete';

function cacheRoot() {
  if (process.platform === 'darwin') {
    return path.join(os.homedir(), 'Library', 'Caches', 'OneCrewConnect', 'runtime');
  }
  if (process.platform === 'win32') {
    return path.join(
      process.env.LOCALAPPDATA || path.join(os.homedir(), 'AppData', 'Local'),
      'OneCrewConnect',
      'cache',
      'runtime',
    );
  }
  return path.join(
    process.env.XDG_CACHE_HOME || path.join(os.homedir(), '.cache'),
    'onecrew-connect',
    'runtime',
  );
}

function safeRelativePath(value) {
  if (typeof value !== 'string' || !value || value.includes('\\') || value.includes('\0')) {
    throw new Error(`Invalid runtime asset path: ${String(value)}`);
  }
  const normalized = path.posix.normalize(value);
  if (
    normalized !== value ||
    normalized.startsWith('/') ||
    normalized === '.' ||
    normalized.split('/').some((part) => part === '..' || part === '')
  ) {
    throw new Error(`Unsafe runtime asset path: ${value}`);
  }
  return normalized;
}

function destinationPath(root, relative) {
  const safe = safeRelativePath(relative);
  const resolvedRoot = path.resolve(root);
  const resolved = path.resolve(resolvedRoot, ...safe.split('/'));
  if (resolved !== resolvedRoot && !resolved.startsWith(`${resolvedRoot}${path.sep}`)) {
    throw new Error(`Runtime asset escapes extraction root: ${relative}`);
  }
  return resolved;
}

function parseManifest() {
  if (!isSea()) throw new Error('OneCrew SEA launcher must run from a single executable');
  const raw = getAsset(MANIFEST_ASSET, 'utf8');
  const manifest = JSON.parse(raw);
  if (
    !manifest ||
    manifest.version !== 1 ||
    typeof manifest.payloadHash !== 'string' ||
    !/^[a-f0-9]{64}$/.test(manifest.payloadHash) ||
    !Array.isArray(manifest.files) ||
    manifest.files.length === 0
  ) {
    throw new Error('Invalid embedded OneCrew runtime manifest');
  }
  const files = manifest.files.map((entry) => {
    if (!entry || typeof entry !== 'object') throw new Error('Invalid runtime manifest entry');
    const asset = safeRelativePath(entry.asset);
    if (
      entry.path !== asset ||
      typeof entry.hash !== 'string' ||
      !/^[a-f0-9]{64}$/.test(entry.hash)
    ) {
      throw new Error(`Invalid runtime manifest entry for ${asset}`);
    }
    if (!Number.isSafeInteger(entry.size) || entry.size < 0) {
      throw new Error(`Invalid runtime manifest size for ${asset}`);
    }
    return { asset, hash: entry.hash, size: entry.size };
  });
  const sorted = [...files].sort((a, b) => a.asset.localeCompare(b.asset));
  if (JSON.stringify(files) !== JSON.stringify(sorted))
    throw new Error('Runtime manifest is not sorted');
  return { payloadHash: manifest.payloadHash, files };
}

async function isComplete(root, payloadHash) {
  try {
    const marker = await readFile(path.join(root, COMPLETE_MARKER), 'utf8');
    return marker.trim() === payloadHash;
  } catch {
    return false;
  }
}

async function extractRuntime(manifest) {
  const base = cacheRoot();
  const target = path.join(base, manifest.payloadHash);
  await mkdir(base, { recursive: true, mode: 0o700 });
  await chmod(base, 0o700);

  if (await isComplete(target, manifest.payloadHash)) return target;

  const temporary = await mkdtemp(path.join(base, '.onecrew-runtime-'));
  try {
    await chmod(temporary, 0o700);
    for (const file of manifest.files) {
      const destination = destinationPath(temporary, file.asset);
      await mkdir(path.dirname(destination), { recursive: true, mode: 0o700 });
      const data = Buffer.from(getAsset(file.asset));
      if (data.byteLength !== file.size)
        throw new Error(`Runtime asset size mismatch: ${file.asset}`);
      const hash = createHash('sha256').update(data).digest('hex');
      if (hash !== file.hash) throw new Error(`Runtime asset hash mismatch: ${file.asset}`);
      await writeFile(destination, data, { mode: 0o600 });
    }
    await writeFile(path.join(temporary, COMPLETE_MARKER), `${manifest.payloadHash}\n`, {
      mode: 0o600,
    });
    try {
      await rename(temporary, target);
    } catch (error) {
      if (error && (error.code === 'EEXIST' || error.code === 'ENOTEMPTY')) {
        if (await isComplete(target, manifest.payloadHash)) return target;
        await rm(target, { recursive: true, force: true });
        await rename(temporary, target);
      } else {
        throw error;
      }
    }
    return target;
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

async function main() {
  const manifest = parseManifest();
  const root = await extractRuntime(manifest);
  const entry = path.join(root, 'dist', 'cli.js');
  await access(entry, constants.R_OK);
  const loadRuntime = createRequire(path.join(root, 'runtime-loader.cjs'));
  await loadRuntime('./runtime-loader.cjs');
}

main().catch((error) => {
  process.stderr.write(
    `${error instanceof Error ? error.stack || error.message : String(error)}\n`,
  );
  process.exitCode = 1;
});
