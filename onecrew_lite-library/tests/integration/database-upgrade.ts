import assert from 'node:assert/strict';
import { PGlite } from '@electric-sql/pglite';
import { inspectLibraryMigrations } from '../../src/database-upgrade.js';

const provider = {
  async getMigrations() {
    return { '001_initial': { async up() {} }, '002_projection': { async up() {} } };
  },
};
const source = await PGlite.create();
try {
  assert.deepEqual(await inspectLibraryMigrations(source, provider), {
    state: 'fresh',
    applied: [],
    pending: ['001_initial', '002_projection'],
  });
  assert.equal(
    (await source.query("SELECT * FROM pg_tables WHERE schemaname='public'")).rows.length,
    0,
  );
  await source.exec('CREATE TABLE customer_notes (note text)');
  await assert.rejects(
    inspectLibraryMigrations(source, provider),
    /library_migration_history_missing/,
  );
  await source.exec('CREATE TABLE kysely_migration (name text PRIMARY KEY, timestamp text)');
  await assert.rejects(
    inspectLibraryMigrations(source, provider),
    /library_migration_history_empty/,
  );
  await source.exec("INSERT INTO kysely_migration VALUES ('001_initial', 'before')");
  assert.deepEqual(await inspectLibraryMigrations(source, provider), {
    state: 'upgrade_required',
    applied: ['001_initial'],
    pending: ['002_projection'],
  });
  await source.exec("INSERT INTO customer_notes VALUES ('Keep the customer note')");
  const ledger = (await source.query('SELECT * FROM kysely_migration ORDER BY name')).rows;
  await inspectLibraryMigrations(source, provider);
  assert.deepEqual(
    (await source.query('SELECT * FROM kysely_migration ORDER BY name')).rows,
    ledger,
  );
  assert.deepEqual((await source.query('SELECT * FROM customer_notes')).rows, [
    { note: 'Keep the customer note' },
  ]);
  await source.exec("INSERT INTO kysely_migration VALUES ('003_unknown', 'later')");
  await assert.rejects(
    inspectLibraryMigrations(source, provider),
    /library_migration_history_unsupported/,
  );
  await source.exec(
    "DELETE FROM kysely_migration WHERE name='003_unknown'; INSERT INTO kysely_migration VALUES ('002_projection', 'later')",
  );
  assert.equal((await inspectLibraryMigrations(source, provider)).state, 'current');
  await source.exec("DELETE FROM kysely_migration WHERE name='001_initial'");
  await assert.rejects(
    inspectLibraryMigrations(source, provider),
    /library_migration_history_unsupported/,
  );
  console.log('Library migration read-only preflight passed');
} finally {
  await source.close();
}
