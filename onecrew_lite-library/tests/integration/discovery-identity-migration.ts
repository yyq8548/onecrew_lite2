import assert from 'node:assert/strict';
import { mkdtemp, readFile, readdir, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { PGlite } from '@electric-sql/pglite';
import { PGliteDialect } from 'kysely-pglite-dialect';
import { Kysely } from 'kysely';
import { databaseMigrationProvider } from '../../src/database-schema.js';
import { migrateLibraryDatabase } from '../../src/database-migrate.js';
import { inspectLibraryMigrations } from '../../src/database-upgrade.js';

const root = await mkdtemp(path.join(os.tmpdir(), 'onecrew-identity-upgrade-'));
const client = await PGlite.create(path.join(root, 'pglite'));
const db = new Kysely({ dialect: new PGliteDialect(client) });
const migrations = await databaseMigrationProvider.getMigrations();
const predecessor = {
  async getMigrations() {
    return {
      '001_tenant_initial': migrations['001_tenant_initial'],
      '002_discovery_projection': migrations['002_discovery_projection'],
    };
  },
};
const preserved = async (engine: PGlite) => {
  const tables = [
    'onecrew_tenants',
    'onecrew_discovery_tables',
    'onecrew_discovery_fields',
    'onecrew_discovery_rows',
    'customer_manual_cells',
    'onecrew_access_accounts',
  ];
  return Promise.all(
    tables.map(
      async (table) => (await engine.query<Record<string, unknown>>(`SELECT * FROM ${table}`)).rows,
    ),
  );
};
const structure = async (engine: PGlite) => ({
  columns: (
    await engine.query(`SELECT table_name,column_name,data_type,is_nullable
    FROM information_schema.columns WHERE table_schema='public'
    AND table_name LIKE 'onecrew_discovery_%' ORDER BY table_name,ordinal_position`)
  ).rows,
  constraints: (
    await engine.query(`SELECT c.conname,pg_get_constraintdef(c.oid) AS definition
    FROM pg_constraint c JOIN pg_class t ON c.conrelid=t.oid
    WHERE t.relname LIKE 'onecrew_discovery_%' ORDER BY c.conname`)
  ).rows,
});
try {
  await migrateLibraryDatabase(client, db, predecessor, root);
  await client.exec(`
    INSERT INTO onecrew_tenants VALUES ('tenant','user','space','base','before','before');
    INSERT INTO onecrew_discovery_tables VALUES ('table','tenant','campaign',1,'before');
    INSERT INTO onecrew_discovery_fields VALUES ('table','field','qualification');
    INSERT INTO onecrew_discovery_rows VALUES
      ('table','row','hit',1,2,'candidate','native','qualified','[]',repeat('a',64),'before');
    INSERT INTO onecrew_access_accounts VALUES ('user','{"synthetic":true}',true,'before','before');
    CREATE TABLE customer_manual_cells (id text PRIMARY KEY, notes text);
    INSERT INTO customer_manual_cells VALUES ('row','Keep this customer note');
  `);
  const before = await preserved(client);
  const latestRows = before.map((rows, index) =>
    index === 3 ? rows.map((row) => ({ ...row, contacts: [] })) : rows,
  );
  const inspection = await inspectLibraryMigrations(client, databaseMigrationProvider);
  assert.deepEqual(inspection.pending, ['003_discovery_identity', '004_discovery_contacts']);
  assert.deepEqual(await preserved(client), before);
  const failing = {
    async getMigrations() {
      return {
        ...migrations,
        '003_discovery_identity': {
          async up(transaction: Parameters<(typeof migrations)[string]['up']>[0]) {
            await migrations['003_discovery_identity'].up(transaction);
            throw new Error('fixture failure after DDL');
          },
        },
      };
    },
  };
  await assert.rejects(
    migrateLibraryDatabase(client, db, failing, root),
    /library_upgrade_failed_before_commit/,
  );
  assert.deepEqual(await inspectLibraryMigrations(client, databaseMigrationProvider), inspection);
  assert.deepEqual(await preserved(client), before);
  assert.equal(
    (
      await client.query<{ name: string | null }>(
        "SELECT to_regclass('onecrew_discovery_identity_snapshots')::text AS name",
      )
    ).rows[0].name,
    null,
  );
  const backups = (await readdir(root)).filter((name) => name.startsWith('recovery-'));
  assert.equal(backups.length, 1);
  const restored = await PGlite.create({
    loadDataDir: new Blob([await readFile(path.join(root, backups[0], 'database.tar.gz'))]),
  });
  try {
    assert.deepEqual(await preserved(restored), before);
    assert.deepEqual(
      await inspectLibraryMigrations(restored, databaseMigrationProvider),
      inspection,
    );
  } finally {
    await restored.close();
  }
  assert.equal(
    await migrateLibraryDatabase(client, db, databaseMigrationProvider, root),
    'upgraded',
  );
  assert.deepEqual(await preserved(client), latestRows);
  assert.equal(
    (await client.query('SELECT * FROM onecrew_discovery_identity_snapshots')).rows.length,
    0,
  );
  const fresh = await PGlite.create();
  const freshDb = new Kysely({ dialect: new PGliteDialect(fresh) });
  try {
    await migrateLibraryDatabase(fresh, freshDb, databaseMigrationProvider, root);
    assert.deepEqual(await structure(client), await structure(fresh));
  } finally {
    await freshDb.destroy();
  }
  const insert = (
    table = 'table',
    format = 1,
    revision = '1',
    hash = 'b'.repeat(64),
    payload = '{}',
  ) =>
    client.query(
      `INSERT INTO onecrew_discovery_identity_snapshots VALUES ($1,$2,1,2,$3,$4,$3,$5,'now')`,
      [table, format, hash, revision, payload],
    );
  await assert.rejects(insert('unknown'));
  await assert.rejects(insert('table', 2));
  await assert.rejects(insert('table', 1, '0'));
  await assert.rejects(insert('table', 1, '9007199254740992'));
  await assert.rejects(insert('table', 1, '1', 'invalid'));
  await assert.rejects(insert('table', 1, '1', 'b'.repeat(64), '[]'));
  await insert();
  await assert.rejects(insert());
  await db.destroy();
  const reopened = await PGlite.create(path.join(root, 'pglite'));
  const reopenedDb = new Kysely({ dialect: new PGliteDialect(reopened) });
  try {
    assert.equal(
      await migrateLibraryDatabase(reopened, reopenedDb, databaseMigrationProvider, root),
      'current',
    );
    assert.deepEqual(await preserved(reopened), latestRows);
    assert.equal(
      (await reopened.query('SELECT * FROM onecrew_discovery_identity_snapshots')).rows.length,
      1,
    );
    await reopened.exec("INSERT INTO kysely_migration VALUES ('004_unknown_future','future')");
    await assert.rejects(
      inspectLibraryMigrations(reopened, databaseMigrationProvider),
      /library_migration_history_unsupported/,
    );
  } finally {
    await reopenedDb.destroy();
  }
  console.log(
    'Library 002→003 preservation, rollback, restored backup, fresh equivalence, constraints and restart passed',
  );
} finally {
  if (!client.closed) await db.destroy();
  await rm(root, { recursive: true, force: true });
}
