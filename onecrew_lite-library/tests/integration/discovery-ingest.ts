import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { sql } from 'kysely';
import { bootstrap } from '../../src/bootstrap.js';
import type { DiscoveryIngest } from '../../src/modules/tables/discovery-projection.js';
import { TestAccess } from '../helpers/test-access.js';

const alice = { id: 'ingest-alice', email: 'alice@example.test' };
const root = await mkdtemp(path.join(os.tmpdir(), 'onecrew-discovery-ingest-'));
let runtime = await bootstrap(root, new TestAccess(alice));
try {
  const ctx = await runtime.tenants.resolve(alice);
  const bob = await runtime.tenants.resolve({ id: 'ingest-bob', email: 'bob@example.test' });
  const managedInput = {
    campaign_id: 'atomic-table-campaign',
    name: 'Campaign sources',
    columns: [{ name: 'Notes', data_type: 'string' as const }],
  };
  const tablesBefore = await runtime.data.listTables(ctx, 1, 100);
  await sql
    .raw(
      `CREATE FUNCTION reject_table_fixture() RETURNS trigger LANGUAGE plpgsql AS $$
    BEGIN RAISE EXCEPTION 'fixture table binding failure'; END; $$;`,
    )
    .execute(runtime.database.db);
  await sql
    .raw(
      `CREATE TRIGGER reject_table_fixture BEFORE INSERT ON onecrew_discovery_tables
    FOR EACH ROW EXECUTE FUNCTION reject_table_fixture();`,
    )
    .execute(runtime.database.db);
  await assert.rejects(runtime.data.ensureDiscoveryCampaignTable(ctx, managedInput));
  assert.deepEqual(await runtime.data.listTables(ctx, 1, 100), tablesBefore);
  assert.equal(await runtime.data.getDiscoveryCampaignTable(ctx, managedInput.campaign_id), null);
  await sql
    .raw('DROP TRIGGER reject_table_fixture ON onecrew_discovery_tables;')
    .execute(runtime.database.db);
  await sql.raw('DROP FUNCTION reject_table_fixture();').execute(runtime.database.db);
  const [managed, concurrent] = await Promise.all([
    runtime.data.ensureDiscoveryCampaignTable(ctx, managedInput),
    runtime.data.ensureDiscoveryCampaignTable(ctx, { ...managedInput, name: 'Retry display name' }),
  ]);
  assert.equal(concurrent.id, managed.id);
  assert.equal(concurrent.name, managed.name);
  assert.equal(managed.columns.length, 7);
  assert.equal(managed.columns.filter((column) => column.read_only).length, 6);
  assert.equal(
    (await runtime.data.getDiscoveryCampaignTable(ctx, managedInput.campaign_id))?.id,
    managed.id,
  );
  assert.equal(await runtime.data.getDiscoveryCampaignTable(bob, managedInput.campaign_id), null);
  const scored = await runtime.data.ensureDiscoveryCampaignTable(ctx, {
    ...managedInput,
    columns: [...managedInput.columns, { name: 'Match evaluation', data_type: 'score' as const }],
  });
  assert.equal(scored.id, managed.id);
  assert.equal(
    scored.columns.find((column) => column.name === 'Notes')?.id,
    managed.columns.find((column) => column.name === 'Notes')?.id,
  );
  const score = scored.columns.find((column) => column.name === 'Match evaluation')!.id;
  const scoreBatch: DiscoveryIngest = {
    table_id: scored.id,
    campaign_id: managedInput.campaign_id,
    criteria_revision: 1,
    source_revision: 1,
    rows: [
      {
        search_hit_id: 'scored-source',
        candidate_id: 'scored-author',
        native_author_id: 'native-scored',
        verdict: 'qualified',
        reasons: [],
        contacts: [{ kind: 'email', original_value: 'hello@example.test', normalized_value: 'hello@example.test',
          source_url: 'https://example.test/creator', source_excerpt: 'Business: hello@example.test',
          captured_at: 100, native_author_id: 'native-scored', attribution: 'author_published' }],
        evidence_version: 'a'.repeat(64),
        values: {
          [score]: {
            level: 'good',
            reason: 'Saved original content supports clear initial relevance.',
          },
        },
      },
    ],
  };
  await runtime.data.ingestDiscoverySources(ctx, scoreBatch);
  const readContacts = async () => (await sql<{ contacts: unknown }>`
    SELECT contacts FROM onecrew_discovery_rows WHERE table_id=${scored.id}
  `.execute(runtime.database.db)).rows[0].contacts;
  assert.deepEqual(await readContacts(), scoreBatch.rows[0].contacts);
  const contactRetry = await runtime.data.ingestDiscoverySources(ctx, scoreBatch);
  assert.equal(contactRetry.inserted_rows, 0);
  assert.equal(contactRetry.updated_rows, 0);
  await assert.rejects(runtime.data.ingestDiscoverySources(ctx, {
    ...scoreBatch, rows: [{ ...scoreBatch.rows[0], contacts: [{ ...scoreBatch.rows[0].contacts[0], native_author_id: 'other-author' }] }],
  }));
  await assert.rejects(runtime.data.ingestDiscoverySources(ctx, {
    ...scoreBatch, rows: [{ ...scoreBatch.rows[0], contacts: [] }],
  }), (error: unknown) => error instanceof Error && 'code' in error && error.code === 'data.projection_version_conflict');
  assert.deepEqual(await readContacts(), scoreBatch.rows[0].contacts);
  assert.deepEqual(
    (await runtime.data.queryRows(ctx, scored.id, 1, 100)).rows[0].values[score],
    scoreBatch.rows[0].values[score],
  );
  const low = { level: 'low' as const, reason: 'An observed required condition is contradicted.' };
  await runtime.data.ingestDiscoverySources(ctx, {
    ...scoreBatch,
    source_revision: 2,
    rows: [
      {
        ...scoreBatch.rows[0],
        verdict: 'rejected',
        reasons: ['initial_match_low'],
        evidence_version: 'b'.repeat(64),
        values: { [score]: low },
      },
    ],
  });
  assert.deepEqual(
    (await runtime.data.queryRows(ctx, scored.id, 1, 100)).rows[0].values[score],
    low,
  );
  await runtime.data.ingestDiscoverySources(ctx, {
    ...scoreBatch,
    source_revision: 3,
    rows: [
      {
        ...scoreBatch.rows[0],
        verdict: 'pending',
        reasons: ['initial_match_unrated'],
        evidence_version: 'c'.repeat(64),
        values: { [score]: null },
      },
    ],
  });
  assert.equal((await runtime.data.queryRows(ctx, scored.id, 1, 100)).rows[0].values[score], null);
  const pendingRowId = (await runtime.data.queryRows(ctx, scored.id, 1, 100)).rows[0].id;
  const manuallyApproved: DiscoveryIngest = {
    ...scoreBatch, source_revision: 4,
    rows: [{ ...scoreBatch.rows[0], verdict: 'qualified',
      reasons: ['manual_approved', 'language_unverified'], evidence_version: 'd'.repeat(64),
      values: { [score]: null } }],
  };
  const manualFirst = await runtime.data.ingestDiscoverySources(ctx, manuallyApproved);
  assert.equal(manualFirst.inserted_rows, 0);
  assert.equal(manualFirst.updated_rows, 1);
  const manualRetry = await runtime.data.ingestDiscoverySources(ctx, manuallyApproved);
  assert.equal(manualRetry.inserted_rows, 0);
  assert.equal(manualRetry.updated_rows, 0);
  const manualRead = await runtime.data.queryRows(ctx, scored.id, 1, 100);
  assert.equal(manualRead.total, 1);
  assert.equal(manualRead.rows[0].id, pendingRowId);
  assert.equal(manualRead.rows[0].values[score], null);
  const manualState = await runtime.data.getDiscoveryState(ctx, {table_id: scored.id,
    campaign_id: managedInput.campaign_id, limit: 100});
  assert.equal(manualState.rows[0].verdict, 'qualified');
  assert.deepEqual(manualState.rows[0].reasons, ['manual_approved', 'language_unverified']);
  const bobManaged = await runtime.data.ensureDiscoveryCampaignTable(bob, managedInput);
  assert.notEqual(bobManaged.id, managed.id);
  const table = await runtime.data.createTable(ctx, 'Managed discovery', [
    { name: 'Notes', data_type: 'string' },
    { name: 'Source URL', data_type: 'website' },
  ]);
  const notes = table.columns[0].id;
  const source = table.columns[1].id;
  const batch: DiscoveryIngest = {
    table_id: table.id,
    campaign_id: 'creator-campaign',
    criteria_revision: 1,
    source_revision: 1,
    rows: [0, 1].map((i) => ({
      search_hit_id: `hit-${i}`,
      candidate_id: null,
      native_author_id: null,
      verdict: 'unverified',
      reasons: ['author_candidate_binding_unverified'],
      contacts: [],
      evidence_version: 'a'.repeat(64),
      values: { [notes]: 'initial', [source]: `https://example.test/post/${i}` },
    })),
  };
  const read = () => runtime.data.queryRows(ctx, table.id, 1, 100);
  const ledger = async () =>
    (
      await sql`
    SELECT * FROM onecrew_discovery_rows WHERE table_id = ${table.id} ORDER BY row_id
  `.execute(runtime.database.db)
    ).rows;

  // Fail after actual engine row creation, before metadata can commit.
  await sql
    .raw(
      `CREATE FUNCTION reject_ingest_fixture() RETURNS trigger LANGUAGE plpgsql AS $$
    BEGIN RAISE EXCEPTION 'fixture metadata failure'; END; $$;`,
    )
    .execute(runtime.database.db);
  await sql
    .raw(
      `CREATE TRIGGER reject_ingest_fixture BEFORE INSERT ON onecrew_discovery_rows
    FOR EACH ROW EXECUTE FUNCTION reject_ingest_fixture();`,
    )
    .execute(runtime.database.db);
  await assert.rejects(runtime.data.ingestDiscoverySources(ctx, batch));
  assert.equal((await read()).total, 0);
  assert.deepEqual((await runtime.data.getTable(ctx, table.id)).columns, table.columns);
  assert.deepEqual(await ledger(), []);
  await sql
    .raw('DROP TRIGGER reject_ingest_fixture ON onecrew_discovery_rows;')
    .execute(runtime.database.db);
  await sql.raw('DROP FUNCTION reject_ingest_fixture();').execute(runtime.database.db);

  const first = await runtime.data.ingestDiscoverySources(ctx, batch);
  assert.equal(first.inserted_rows, 2);
  assert.equal(first.rows.length, 2);
  assert.equal(new Set(first.rows.map((row) => row.row_id)).size, 2);
  const createdRows = (await read()).rows;
  for (const binding of first.rows) {
    const expected = batch.rows.find((entry) => entry.search_hit_id === binding.search_hit_id)!;
    assert.equal(
      createdRows.find((entry) => entry.id === binding.row_id)!.values[source],
      expected.values[source],
    );
  }
  const original = await ledger();
  const row = first.rows.find((row) => row.search_hit_id === 'hit-0')!;
  await runtime.data.updateRows(ctx, table.id, [
    { id: row.row_id, values: { [notes]: 'customer edited' } },
  ]);
  const repeated = await runtime.data.ingestDiscoverySources(ctx, batch);
  assert.equal(repeated.inserted_rows, 0);
  const firstPage = await runtime.data.getDiscoveryState(ctx, {
    table_id: table.id,
    campaign_id: batch.campaign_id,
    limit: 1,
  });
  assert.equal(firstPage.rows.length, 1);
  assert.equal(firstPage.has_more, true);
  const secondPage = await runtime.data.getDiscoveryState(ctx, {
    table_id: table.id,
    campaign_id: batch.campaign_id,
    limit: 1,
    after_hit_id: firstPage.rows[0].search_hit_id,
  });
  assert.equal(secondPage.has_more, false);
  assert.equal(secondPage.rows.length, 1);
  assert.notEqual(firstPage.rows[0].row_id, secondPage.rows[0].row_id);
  assert.equal(repeated.updated_rows, 0);
  assert.deepEqual(repeated.rows, first.rows);
  assert.deepEqual(await ledger(), original);
  assert.equal(
    (await read()).rows.find((entry) => entry.id === row.row_id)!.values[notes],
    'customer edited',
  );
  await assert.rejects(runtime.data.ingestDiscoverySources(bob, batch), { status: 404 });
  await assert.rejects(
    runtime.data.ingestDiscoverySources(ctx, { ...batch, campaign_id: 'wrong' }),
    { code: 'data.projection_table_conflict' },
  );

  // A newly created row rolls back when another row's source version conflicts.
  await assert.rejects(
    runtime.data.ingestDiscoverySources(ctx, {
      ...batch,
      rows: [
        { ...batch.rows[0], search_hit_id: 'new-hit' },
        { ...batch.rows[1], evidence_version: 'b'.repeat(64) },
      ],
    }),
    { code: 'data.projection_version_conflict' },
  );
  assert.equal((await read()).total, 2);
  assert.deepEqual(await ledger(), original);
  const enriched = await runtime.data.ingestDiscoverySources(ctx, {
    ...batch,
    source_revision: 2,
    rows: batch.rows.map((entry) => ({
      ...entry,
      verdict: 'pending',
      candidate_id: `candidate-${entry.search_hit_id}`,
      native_author_id: `native-${entry.search_hit_id}`,
      evidence_version: 'b'.repeat(64),
    })),
  });
  assert.equal(enriched.inserted_rows, 0);
  assert.equal(enriched.updated_rows, 2);
  assert.equal(
    (await read()).rows.find((entry) => entry.id === row.row_id)!.values[notes],
    'customer edited',
  );

  const legacy = await runtime.data.createTable(ctx, 'Legacy with editable source text', [
    { name: 'Search hit ID', data_type: 'string' },
  ]);
  await runtime.data.createRows(ctx, legacy.id, [{ values: { [legacy.columns[0].id]: 'hit-0' } }]);
  await assert.rejects(
    runtime.data.ingestDiscoverySources(ctx, {
      ...batch,
      table_id: legacy.id,
      campaign_id: 'legacy-campaign',
      rows: [{ ...batch.rows[0], values: { [legacy.columns[0].id]: 'hit-0' } }],
    }),
    { code: 'data.projection_unmanaged_table' },
  );
  assert.equal((await runtime.data.queryRows(ctx, legacy.id, 1, 10)).total, 1);

  await runtime.data.close();
  runtime = await bootstrap(root, new TestAccess(alice));
  const resumed = await runtime.tenants.resolve(alice);
  assert.equal(
    (await runtime.data.ensureDiscoveryCampaignTable(resumed, managedInput)).id,
    managed.id,
  );
  assert.equal(
    (await runtime.data.getDiscoveryCampaignTable(resumed, managedInput.campaign_id))?.id,
    managed.id,
  );
  const restart = await runtime.data.ingestDiscoverySources(resumed, {
    ...batch,
    source_revision: 2,
    rows: batch.rows.map((entry) => ({
      ...entry,
      verdict: 'pending',
      candidate_id: `candidate-${entry.search_hit_id}`,
      native_author_id: `native-${entry.search_hit_id}`,
      evidence_version: 'b'.repeat(64),
    })),
  });
  assert.equal(restart.inserted_rows, 0);
  assert.equal(restart.updated_rows, 0);
  assert.deepEqual(restart.rows, first.rows);
  await runtime.data.deleteRows(resumed, table.id, [row.row_id]);
  await assert.rejects(
    runtime.data.getDiscoveryState(resumed, {
      table_id: table.id,
      campaign_id: batch.campaign_id,
      limit: 100,
    }),
    { code: 'data.projection_row_missing' },
  );
  await assert.rejects(
    runtime.data.ingestDiscoverySources(resumed, {
      ...batch,
      source_revision: 3,
      rows: [
        {
          ...batch.rows[0],
          verdict: 'pending',
          candidate_id: 'candidate-hit-0',
          native_author_id: 'native-hit-0',
        },
        { ...batch.rows[1], search_hit_id: 'another-new-hit' },
      ],
    }),
    { code: 'data.projection_row_missing' },
  );
  assert.equal((await read()).total, 1);
  console.log(
    'Discovery ingest: atomic creation/binding, retry, notes, tenant isolation, legacy refusal and restart PASS',
  );
} finally {
  await runtime.data.close();
  await rm(root, { recursive: true, force: true });
}
