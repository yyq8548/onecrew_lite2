import assert from 'node:assert/strict';
import { PGlite } from '@electric-sql/pglite';
import { PGliteDialect } from 'kysely-pglite-dialect';
import { Kysely, Migrator } from 'kysely';
import { databaseMigrationProvider } from '../../src/database-schema.js';

// Capability proof for the startup recovery path. In-memory synthetic data only:
// never open, copy, migrate or restore the user's running Library directory.
const source = await PGlite.create();
const db = new Kysely({ dialect: new PGliteDialect(source) });
let restored: PGlite | undefined;
try {
  const migration = await new Migrator({
    db,
    provider: databaseMigrationProvider,
  }).migrateToLatest();
  assert.equal(migration.error, undefined);
  await source.exec(`
    CREATE TABLE recovery_probe (id text PRIMARY KEY, value jsonb NOT NULL);
    INSERT INTO recovery_probe VALUES ('manual', '{"status":"Customer reviewed","note":"Keep me"}');
    INSERT INTO onecrew_tenants VALUES ('tenant', 'user', 'space', 'base', 'before', 'before');
    INSERT INTO onecrew_field_types VALUES ('table', 'field', 'string');
  `);
  await assert.rejects(
    source.transaction(async (tx) => {
      await tx.exec("INSERT INTO recovery_probe VALUES ('rolled-back', '{}')");
      throw new Error('synthetic rollback');
    }),
    /synthetic rollback/,
  );
  const original = await source.query('SELECT * FROM recovery_probe ORDER BY id');
  const ledger = await source.query('SELECT * FROM kysely_migration ORDER BY name');
  const archive = await source.dumpDataDir('gzip');
  assert.ok(archive.size > 0);

  // A later source mutation must not appear in the already captured recovery point.
  await source.exec(
    'UPDATE recovery_probe SET value = \'{"note":"after snapshot"}\' WHERE id=\'manual\'',
  );
  restored = await PGlite.create({ loadDataDir: archive });
  assert.deepEqual(
    (await restored.query('SELECT * FROM recovery_probe ORDER BY id')).rows,
    original.rows,
  );
  assert.deepEqual(
    (await restored.query('SELECT * FROM kysely_migration ORDER BY name')).rows,
    ledger.rows,
  );
  assert.deepEqual(
    (await restored.query('SELECT * FROM onecrew_tenants')).rows,
    (await source.query('SELECT * FROM onecrew_tenants')).rows,
  );
  assert.deepEqual(
    (await restored.query('SELECT * FROM onecrew_field_types')).rows,
    (await source.query('SELECT * FROM onecrew_field_types')).rows,
  );
  assert.notDeepEqual(
    (await restored.query('SELECT * FROM recovery_probe')).rows,
    (await source.query('SELECT * FROM recovery_probe')).rows,
  );
  // Restoration is explicit and isolated, never an automatic source rollback.
  await restored.exec("INSERT INTO recovery_probe VALUES ('restored-only', '{}')");
  assert.equal(
    (await source.query("SELECT id FROM recovery_probe WHERE id='restored-only'")).rows.length,
    0,
  );
  console.log('PGlite recovery snapshot capability passed (synthetic memory databases only)');
} finally {
  if (restored) await restored.close();
  await db.destroy();
}
