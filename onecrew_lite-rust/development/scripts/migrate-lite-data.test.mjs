import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, readFile, rm, symlink, access, open } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { createRequire } from 'node:module';
const require = createRequire(new URL('../../../onecrew_lite-library/package.json', import.meta.url));
const { tryLock } = require('fs-native-extensions');
import { copyLegacyData } from './migrate-lite-data.mjs';
test('Lite copy preserves source, skips runtime state and never overwrites', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'lite-migration-'));
  try {
    const source = path.join(root, 'advanced'), target = path.join(root, 'lite');
    await mkdir(source); await mkdir(path.join(source, 'run'));
    await writeFile(path.join(source, 'crm.db'), 'saved-fixture');
    await writeFile(path.join(source, 'run/server.json'), 'old-runtime');
    await writeFile(path.join(source, 'database.lock'), '');
    const lock = await open(path.join(source, 'database.lock'), 'r+');
    assert.equal(tryLock(lock.fd), true);
    await assert.rejects(copyLegacyData(source, target), /running/);
    await lock.close();
    await copyLegacyData(source, target);
    assert.equal(await readFile(path.join(target, 'crm.db'), 'utf8'), 'saved-fixture');
    assert.equal(await readFile(path.join(source, 'crm.db'), 'utf8'), 'saved-fixture');
    await assert.rejects(access(path.join(target, 'run')));
    await assert.rejects(copyLegacyData(source, target), /already exists/);
    await assert.rejects(copyLegacyData(source, path.join(source, 'nested')), /separate/);
    await symlink(path.join(source, 'crm.db'), path.join(source, 'unsafe'));
    await assert.rejects(copyLegacyData(source, path.join(root, 'unsafe-copy')), /symbolic/);
    await assert.rejects(access(path.join(root, 'unsafe-copy')));
  } finally { await rm(root, { recursive: true, force: true }); }
});
