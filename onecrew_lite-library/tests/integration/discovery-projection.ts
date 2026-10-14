import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { sql } from 'kysely';
import { bootstrap } from '../../src/bootstrap.js';
import type { DiscoveryProjection } from '../../src/modules/tables/discovery-projection.js';
import { TestAccess } from '../helpers/test-access.js';

const alice = { id: 'projection-writer-alice', email: 'alice@example.test' };
const root = await mkdtemp(path.join(os.tmpdir(), 'onecrew-projection-writer-'));
let runtime = await bootstrap(root, new TestAccess(alice));
try {
  const ctx = await runtime.tenants.resolve(alice);
  const bob = await runtime.tenants.resolve({
    id: 'projection-writer-bob',
    email: 'bob@example.test',
  });
  const table = await runtime.data.createTable(ctx, 'Discovery', [
    { name: 'Notes', data_type: 'string' },
  ]);
  const manual = table.columns[0].id;
  const rows = await runtime.data.createRows(ctx, table.id, [
    { values: { [manual]: 'manual first' } },
    { values: { [manual]: 'manual second' } },
  ]);
  const snapshot: DiscoveryProjection = {
    table_id: table.id,
    campaign_id: 'synthetic-campaign',
    criteria_revision: 1,
    source_revision: 1,
    rows: rows.map((row, i) => ({
      row_id: row.id,
      search_hit_id: `hit-${i}`,
      candidate_id: `candidate-${i}`,
      native_author_id: `native-${i}`,
      verdict: 'pending',
      reasons: ['Demo not verified'],
      contacts: [],
      evidence_version: 'a'.repeat(64),
    })),
  };
  const ledger = async () =>
    (
      await sql`
    SELECT * FROM onecrew_discovery_rows WHERE table_id = ${table.id} ORDER BY row_id
  `.execute(runtime.database.db)
    ).rows;
  const read = () => runtime.data.queryRows(ctx, table.id, 1, 100);
  // Fail after field creation: metadata, Teable DDL and values must all roll back.
  const other = await runtime.data.createTable(ctx, 'Other', [
    { name: 'Notes', data_type: 'string' },
  ]);
  const [otherRow] = await runtime.data.createRows(ctx, other.id, [
    { values: { [other.columns[0].id]: 'other' } },
  ]);
  await assert.rejects(
    runtime.data.applyDiscoveryProjection(ctx, {
      ...snapshot,
      rows: [{ ...snapshot.rows[0], row_id: otherRow.id }],
    }),
    { code: 'data.projection_row_missing' },
  );
  assert.deepEqual((await runtime.data.getTable(ctx, table.id)).columns, table.columns);
  assert.deepEqual(await ledger(), []);
  assert.equal(
    (
      await sql<{
        count: string;
      }>`SELECT count(*)::text AS count FROM onecrew_discovery_tables`.execute(runtime.database.db)
    ).rows[0].count,
    '0',
  );

  const first = await runtime.data.applyDiscoveryProjection(ctx, snapshot);
  assert.equal(first.updated_rows, 2);
  assert.equal(first.unchanged_rows, 0);
  assert.equal(first.table.columns.filter((column) => column.read_only).length, 6);
  const qualification = first.table.columns.find(
    (column) => column.name === 'OneCrew qualification',
  )!.id;
  const saved = await ledger();
  assert.equal((await read()).rows[0].values[qualification], 'pending');
  await runtime.data.updateRows(ctx, table.id, [
    { id: rows[0].id, values: { [manual]: 'customer edited' } },
  ]);
  const repeat = await runtime.data.applyDiscoveryProjection(ctx, snapshot);
  assert.equal(repeat.updated_rows, 0);
  assert.equal(repeat.unchanged_rows, 2);
  assert.deepEqual(await ledger(), saved, 'retry must not even change updated_at');
  await assert.rejects(
    runtime.data.applyDiscoveryProjection(ctx, {
      ...snapshot,
      rows: [snapshot.rows[0], { ...snapshot.rows[1], reasons: ['forged reason'] }],
    }),
    { code: 'data.projection_version_conflict' },
  );
  await assert.rejects(
    runtime.data.applyDiscoveryProjection(ctx, {
      ...snapshot,
      source_revision: 2,
      rows: [{ ...snapshot.rows[0], native_author_id: 'another-native' }],
    }),
    { code: 'data.projection_identity_conflict' },
  );
  await assert.rejects(
    runtime.data.applyDiscoveryProjection(ctx, {
      ...snapshot,
      source_revision: 2,
      rows: [{ ...snapshot.rows[0], row_id: rows[1].id }],
    }),
    { code: 'data.projection_row_conflict' },
  );
  assert.deepEqual(await ledger(), saved);
  await assert.rejects(runtime.data.applyDiscoveryProjection(bob, snapshot), { status: 404 });
  await assert.rejects(
    runtime.data.applyDiscoveryProjection(ctx, { ...snapshot, campaign_id: 'other' }),
    { code: 'data.projection_table_conflict' },
  );
  await assert.rejects(
    runtime.data.applyDiscoveryProjection(ctx, {
      ...snapshot,
      table_id: other.id,
      rows: [{ ...snapshot.rows[0], row_id: otherRow.id }],
    }),
    { code: 'data.projection_table_conflict' },
  );
  await assert.rejects(
    runtime.data.applyDiscoveryProjection(ctx, {
      ...snapshot,
      rows: [snapshot.rows[0], snapshot.rows[0]],
    }),
    { code: 'data.projection_invalid' },
  );

  const next: DiscoveryProjection = {
    ...snapshot,
    source_revision: 2,
    rows: snapshot.rows.map((row) => ({
      ...row,
      verdict: 'rejected',
      reasons: ['Frozen threshold not met'],
      evidence_version: 'b'.repeat(64),
    })),
  };
  const physical = await runtime.database.db
    .selectFrom('table_meta')
    .select('db_table_name')
    .where('id', '=', table.id)
    .executeTakeFirstOrThrow();
  const beforeFault = await read();
  await sql`CREATE FUNCTION public.projection_test_fail() RETURNS trigger
    LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected projection failure'; END $$`.execute(
    runtime.database.db,
  );
  await sql`CREATE TRIGGER projection_test_fail AFTER UPDATE ON ${sql.table(physical.db_table_name)}
    FOR EACH STATEMENT EXECUTE FUNCTION public.projection_test_fail()`.execute(runtime.database.db);
  await assert.rejects(runtime.data.applyDiscoveryProjection(ctx, next));
  assert.deepEqual(await ledger(), saved, 'record write failure must roll back metadata too');
  assert.deepEqual(await read(), beforeFault, 'record write failure must preserve all cells');
  await sql`DROP TRIGGER projection_test_fail ON ${sql.table(physical.db_table_name)}`.execute(
    runtime.database.db,
  );
  await sql`DROP FUNCTION public.projection_test_fail()`.execute(runtime.database.db);
  assert.equal((await runtime.data.applyDiscoveryProjection(ctx, next)).updated_rows, 2);
  await assert.rejects(runtime.data.applyDiscoveryProjection(ctx, snapshot), {
    code: 'data.projection_stale',
  });
  assert.equal(
    (await read()).rows.find((row) => row.id === rows[0].id)?.values[manual],
    'customer edited',
  );
  assert.equal(
    (await read()).rows.find((row) => row.id === rows[0].id)?.values[qualification],
    'rejected',
  );
  await assert.rejects(
    runtime.data.updateRows(ctx, table.id, [
      { id: rows[0].id, values: { [qualification]: 'qualified' } },
    ]),
    { code: 'data.system_field_read_only' },
  );

  // A reserved display name is not proof of ownership; do not overwrite or adopt it.
  const collision = await runtime.data.createTable(ctx, 'Collision', [
    { name: 'OneCrew qualification', data_type: 'string' },
  ]);
  const [collisionRow] = await runtime.data.createRows(ctx, collision.id, [
    { values: { [collision.columns[0].id]: 'customer text' } },
  ]);
  await assert.rejects(
    runtime.data.applyDiscoveryProjection(ctx, {
      ...snapshot,
      table_id: collision.id,
      campaign_id: 'collision',
      rows: [{ ...snapshot.rows[0], row_id: collisionRow.id }],
    }),
    { code: 'data.projection_field_name_conflict' },
  );
  assert.deepEqual(await runtime.data.getTable(ctx, collision.id), collision);
  await runtime.data.close();
  runtime = await bootstrap(root, new TestAccess(alice));
  const reopened = await runtime.tenants.resolve(alice);
  assert.equal((await runtime.data.applyDiscoveryProjection(reopened, next)).unchanged_rows, 2);
  assert.equal(
    (await read()).rows.find((row) => row.id === rows[0].id)?.values[manual],
    'customer edited',
  );
  const persisted = await ledger();
  await runtime.data.deleteRows(reopened, table.id, [rows[1].id]);
  await assert.rejects(
    runtime.data.applyDiscoveryProjection(reopened, { ...next, source_revision: 3 }),
    { code: 'data.projection_row_missing' },
  );
  assert.deepEqual(
    await ledger(),
    persisted,
    'missing row must not commit a partial batch or resurrect it',
  );
  console.log(
    'Discovery projection passed: atomic binding/cells, no-op, conflicts, manual preservation, isolation, restart.',
  );
} finally {
  await runtime.data.close();
  await rm(root, { recursive: true, force: true });
}
