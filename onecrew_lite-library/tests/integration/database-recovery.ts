import assert from 'node:assert/strict';
import { mkdtemp, readFile, readdir, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { PGlite } from '@electric-sql/pglite';
import { PGliteDialect } from 'kysely-pglite-dialect';
import { Kysely, Migrator } from 'kysely';
import { databaseMigrationProvider } from '../../src/database-schema.js';
import { inspectLibraryMigrations } from '../../src/database-upgrade.js';
import { createLibraryRecoveryPoint } from '../../src/database-recovery.js';

const root = await mkdtemp(path.join(os.tmpdir(), 'onecrew-recovery-'));
const source = await PGlite.create();
const db = new Kysely({ dialect: new PGliteDialect(source) });
const future = {
  async getMigrations() {
    return {
      ...(await databaseMigrationProvider.getMigrations()),
      '003_synthetic_test_only': { async up() {} },
    };
  },
};
try {
  assert.equal(
    (await new Migrator({ db, provider: databaseMigrationProvider }).migrateToLatest()).error,
    undefined,
  );
  await source.exec(
    "CREATE TABLE customer_probe (id text, note text); INSERT INTO customer_probe VALUES ('row-one','Preserve manual note')",
  );
  const expected = await inspectLibraryMigrations(source, future);
  assert.equal(expected.state, 'upgrade_required');
  const point = await createLibraryRecoveryPoint(source, future, root, expected);
  assert.match(point.sha256, /^[a-f0-9]{64}$/);
  assert.ok(point.bytes > 0);
  // Independently reopen the actual disk file, not the helper's original Blob.
  const restored = await PGlite.create({ loadDataDir: new Blob([await readFile(point.file)]) });
  try {
    assert.deepEqual((await restored.query('SELECT * FROM customer_probe')).rows, [
      { id: 'row-one', note: 'Preserve manual note' },
    ]);
    assert.deepEqual(await inspectLibraryMigrations(restored, future), expected);
  } finally {
    await restored.close();
  }
  const artifacts = await readdir(root);
  await assert.rejects(
    createLibraryRecoveryPoint(source, future, root, { ...expected, pending: ['wrong'] }),
    /library_recovery_source_changed/,
  );
  assert.deepEqual(await readdir(root), artifacts);
  await assert.rejects(
    createLibraryRecoveryPoint(source, future, path.join(root, 'missing-directory'), expected),
    /library_recovery_directory_invalid/,
  );
  assert.deepEqual(await readdir(root), artifacts);
  assert.deepEqual((await source.query('SELECT * FROM customer_probe')).rows, [
    { id: 'row-one', note: 'Preserve manual note' },
  ]);
  assert.deepEqual(await inspectLibraryMigrations(source, future), expected);
  console.log('Disk recovery point independently verified; source remains unmigrated');
} finally {
  await db.destroy();
  await rm(root, { recursive: true, force: true });
}
