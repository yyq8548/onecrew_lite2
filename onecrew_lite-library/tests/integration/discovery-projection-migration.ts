import assert from 'node:assert/strict';
import { mkdtemp, readdir, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { PGlite } from '@electric-sql/pglite';
import { PGliteDialect } from 'kysely-pglite-dialect';
import { Kysely } from 'kysely';
import { databaseMigrationProvider } from '../../src/database-schema.js';
import { migrateLibraryDatabase } from '../../src/database-migrate.js';

const root = await mkdtemp(path.join(os.tmpdir(), 'onecrew-projection-upgrade-'));
const client = await PGlite.create(path.join(root, 'pglite'));
const db = new Kysely({ dialect: new PGliteDialect(client) });
const fresh = await PGlite.create();
const freshDb = new Kysely({ dialect: new PGliteDialect(fresh) });
const oldProvider = {
  async getMigrations() {
    const migrations = await databaseMigrationProvider.getMigrations();
    return { '001_tenant_initial': migrations['001_tenant_initial'] };
  },
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
  await migrateLibraryDatabase(client, db, oldProvider, root);
  await client.exec(`
    INSERT INTO onecrew_tenants VALUES ('tenant','user','space','base','before','before');
    INSERT INTO onecrew_field_types VALUES ('existing-table','manual-field','string');
    INSERT INTO onecrew_access_accounts VALUES ('user','{"synthetic":true}',true,'before','before');
    CREATE TABLE customer_manual_cells (id text PRIMARY KEY, status text, notes text);
    INSERT INTO customer_manual_cells VALUES ('row','Contact later','Customer note');
  `);
  const preserved = async () =>
    Promise.all([
      client.query('SELECT * FROM onecrew_tenants'),
      client.query('SELECT * FROM onecrew_field_types'),
      client.query('SELECT * FROM onecrew_access_accounts'),
      client.query('SELECT * FROM customer_manual_cells'),
    ]).then((results) => results.map((result) => result.rows));
  const before = await preserved();
  assert.equal(
    await migrateLibraryDatabase(client, db, databaseMigrationProvider, root),
    'upgraded',
  );
  assert.deepEqual(await preserved(), before);
  assert.equal((await readdir(root)).filter((name) => name.startsWith('recovery-')).length, 1);
  await migrateLibraryDatabase(fresh, freshDb, databaseMigrationProvider, root);
  assert.deepEqual(await structure(client), await structure(fresh));
  for (const table of [
    'onecrew_discovery_tables',
    'onecrew_discovery_fields',
    'onecrew_discovery_rows',
  ]) {
    assert.equal((await client.query(`SELECT * FROM ${table}`)).rows.length, 0);
  }
  await client.exec(
    "INSERT INTO onecrew_discovery_tables VALUES ('table','tenant','campaign',1,'now')",
  );
  await client.exec(
    "INSERT INTO onecrew_discovery_fields VALUES ('table','field','qualification')",
  );
  await assert.rejects(
    client.exec(
      "INSERT INTO onecrew_discovery_fields VALUES ('table','other-field','qualification')",
    ),
  );
  await assert.rejects(
    client.exec("INSERT INTO onecrew_discovery_fields VALUES ('table','field','reasons')"),
  );
  await assert.rejects(
    client.exec(
      "INSERT INTO onecrew_discovery_tables VALUES ('other','missing-tenant','campaign',1,'now')",
    ),
  );
  const insert = (
    row: string,
    hit: string,
    verdict: string,
    candidate: string | null,
    author: string | null,
    revision = 1,
  ) =>
    client.query(
      `INSERT INTO onecrew_discovery_rows
      (table_id,row_id,search_hit_id,criteria_revision,source_revision,candidate_id,native_author_id,verdict,reasons,evidence_version,updated_at)
      VALUES ('table',$1,$2,$3,1,$4,$5,$6,'[]',$7,'now')`,
      [row, hit, revision, candidate, author, verdict, 'a'.repeat(64)],
    );
  await insert('row', 'hit', 'unverified', null, null);
  await assert.rejects(insert('other-row', 'hit', 'unverified', null, null));
  await assert.rejects(insert('unbound', 'new-hit', 'qualified', null, null));
  await assert.rejects(insert('wrong-revision', 'new-hit', 'pending', 'candidate', 'native', 0));
  await insert('bound', 'bound-hit', 'pending', 'candidate', 'native');
  await db.destroy();
  const reopened = await PGlite.create(path.join(root, 'pglite'));
  const reopenedDb = new Kysely({ dialect: new PGliteDialect(reopened) });
  try {
    assert.equal(
      await migrateLibraryDatabase(reopened, reopenedDb, databaseMigrationProvider, root),
      'current',
    );
    assert.equal((await reopened.query('SELECT * FROM onecrew_discovery_rows')).rows.length, 2);
    assert.deepEqual((await reopened.query('SELECT * FROM customer_manual_cells')).rows, before[3]);
    assert.equal((await readdir(root)).filter((name) => name.startsWith('recovery-')).length, 1);
  } finally {
    await reopenedDb.destroy();
  }
  console.log(
    'Library 001→latest preservation, fresh equivalence, identity constraints and restart passed',
  );
} finally {
  if (!client.closed) await db.destroy();
  await freshDb.destroy();
  await rm(root, { recursive: true, force: true });
}
