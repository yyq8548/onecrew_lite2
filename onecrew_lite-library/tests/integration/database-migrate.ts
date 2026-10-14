import assert from 'node:assert/strict';
import { mkdtemp, readdir, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { PGlite } from '@electric-sql/pglite';
import { PGliteDialect } from 'kysely-pglite-dialect';
import { Kysely, sql } from 'kysely';
import { databaseMigrationProvider } from '../../src/database-schema.js';
import { inspectLibraryMigrations } from '../../src/database-upgrade.js';
import { migrateLibraryDatabase, requireRecoveryCapacity } from '../../src/database-migrate.js';

const root = await mkdtemp(path.join(os.tmpdir(), 'onecrew-migration-'));
const client = await PGlite.create();
const db = new Kysely({ dialect: new PGliteDialect(client) });
const synthetic = (fail: boolean) => ({
  async getMigrations() {
    return {
      ...(await databaseMigrationProvider.getMigrations()),
        '003_synthetic_test_only': {
        async up(tx: Kysely<unknown>) {
          await sql`CREATE TABLE upgrade_probe (id text PRIMARY KEY)`.execute(tx);
          await sql`INSERT INTO upgrade_probe VALUES ('new-version')`.execute(tx);
          if (fail) throw new Error('synthetic failure body must not leak');
        },
      },
    };
  },
});
try {
  requireRecoveryCapacity(1n, 64n * 1024n * 1024n + 3n);
  assert.throws(() => requireRecoveryCapacity(1n, 0n), /library_recovery_space_insufficient/);
  assert.throws(
    () => requireRecoveryCapacity(0n, 99999999999n),
    /library_recovery_size_unsupported/,
  );
  assert.throws(
    () => requireRecoveryCapacity(512n * 1024n * 1024n + 1n, 99999999999n),
    /library_recovery_size_unsupported/,
  );
  await migrateLibraryDatabase(client, db, databaseMigrationProvider, root);
  assert.deepEqual(await readdir(root), []);
  await client.exec(
    "CREATE TABLE customer_notes (id text, note text); INSERT INTO customer_notes VALUES ('row', 'Keep manual edits')",
  );
  const before = await inspectLibraryMigrations(client, synthetic(true));
  await assert.rejects(
    migrateLibraryDatabase(client, db, synthetic(true), root),
    (error) => error instanceof Error && error.message === 'library_upgrade_failed_before_commit',
  );
  assert.deepEqual(await inspectLibraryMigrations(client, synthetic(true)), before);
  assert.equal(
    (
      await client.query<{ absent: boolean }>(
        "SELECT to_regclass('public.upgrade_probe') IS NULL AS absent",
      )
    ).rows[0].absent,
    true,
  );
  assert.equal((await readdir(root)).length, 1);
  await migrateLibraryDatabase(client, db, synthetic(false), root);
  assert.equal((await inspectLibraryMigrations(client, synthetic(false))).state, 'current');
  assert.deepEqual((await client.query('SELECT * FROM customer_notes')).rows, [
    { id: 'row', note: 'Keep manual edits' },
  ]);
  assert.deepEqual((await client.query('SELECT * FROM upgrade_probe')).rows, [
    { id: 'new-version' },
  ]);
  assert.equal((await readdir(root)).length, 2);
  await migrateLibraryDatabase(client, db, synthetic(false), root);
  assert.equal((await readdir(root)).length, 2); // Current reopen never creates another backup.
  console.log('Backup-first migration rollback, retry, preservation and current no-op passed');
} finally {
  await db.destroy();
  await rm(root, { recursive: true, force: true });
}
