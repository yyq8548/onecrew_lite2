// Explicit offline copy: the advanced edition and its original data remain intact.
import { createRequire } from 'node:module';
import { randomUUID } from 'node:crypto';
import { lstat, mkdir, open, readdir, copyFile, rename, rm } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const require = createRequire(new URL('../../../onecrew_lite-library/package.json', import.meta.url));
const { tryLock } = require('fs-native-extensions');
const transient = new Set(['run', 'server.lock', 'database.lock', '.onecrew-service-owner.lock',
  '.onecrew-service.json', '.onecrew_lite-service-owner.lock', '.onecrew_lite-service.json']);
async function exists(target) {
  try { await lstat(target); return true; } catch (error) { if (error.code === 'ENOENT') return false; throw error; }
}
async function copyTree(source, destination) {
  const stat = await lstat(source);
  if (stat.isSymbolicLink()) throw Error('Migration refuses symbolic links');
  if (stat.isDirectory()) {
    await mkdir(destination, { mode: 0o700 });
    for (const name of await readdir(source)) {
      if (!transient.has(name)) await copyTree(path.join(source, name), path.join(destination, name));
    }
  } else if (stat.isFile()) await copyFile(source, destination);
  else throw Error('Migration refuses special files');
}
export async function copyLegacyData(source, destination) {
  source = path.resolve(source); destination = path.resolve(destination);
  if (destination === source || destination.startsWith(source + path.sep)
      || source.startsWith(destination + path.sep)) throw Error('Source and destination must be separate');
  if (!(await lstat(source)).isDirectory() || (await lstat(source)).isSymbolicLink()) throw Error('Invalid source directory');
  if (await exists(destination)) throw Error('Destination already exists; never overwrite');
  const locks = [];
  const staged = destination + '.migration-' + randomUUID();
  try {
    for (const name of ['.onecrew-service-owner.lock', 'server.lock', 'database.lock']) {
      const target = path.join(source, name);
      if (!(await exists(target))) continue;
      if ((await lstat(target)).isSymbolicLink()) throw Error('Unsafe source lock');
      const handle = await open(target, 'r+'); locks.push(handle);
      if (!tryLock(handle.fd)) throw Error('Source is running; stop both editions before copying');
    }
    await mkdir(path.dirname(destination), { recursive: true, mode: 0o700 });
    await copyTree(source, staged);
    if (await exists(destination)) throw Error('Destination appeared during migration');
    await rename(staged, destination);
  } finally {
    await rm(staged, { recursive: true, force: true });
    for (const handle of locks.reverse()) await handle.close();
  }
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [source, destination, apply] = process.argv.slice(2);
  if (!source || !destination || apply !== '--apply') throw Error('Usage: node migrate-lite-data.mjs SOURCE DESTINATION --apply; stop both editions first');
  await copyLegacyData(source, destination);
  console.log('Offline data copied to Lite. Original data preserved. No service was started.');
}
