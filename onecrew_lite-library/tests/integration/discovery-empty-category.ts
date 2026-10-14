import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { bootstrap } from '../../src/bootstrap.js';
import { TestAccess } from '../helpers/test-access.js';
const user = { id: 'empty-category', email: 'test@example.test' };
const root = await mkdtemp(path.join(os.tmpdir(), 'lite-empty-category-'));
const runtime = await bootstrap(root, new TestAccess(user));
try {
  const ctx = await runtime.tenants.resolve(user);
  const table = await runtime.data.ensureDiscoveryCampaignTable(ctx, { campaign_id: 'mouse', name: 'Mouse', columns: [{ name: 'Content category', data_type: 'string' }] });
  const category = table.columns.find(c => c.name === 'Content category')!.id;
  const result = await runtime.data.ingestDiscoverySources(ctx, { table_id: table.id, campaign_id: 'mouse', criteria_revision: 1, source_revision: 1, rows: [{ search_hit_id: 'hit', candidate_id: null, native_author_id: null, verdict: 'unverified', reasons: [], contacts: [], evidence_version: 'a'.repeat(64), values: { [category]: '' } }] });
  assert.equal(result.inserted_rows, 1);
  const upgraded = await runtime.data.ensureDiscoveryCampaignTable(ctx, { campaign_id: 'mouse', name: 'Mouse', columns: [{ name: 'Content category', data_type: 'string' }, { name: 'Followers', data_type: 'string' }] });
  assert.equal(upgraded.id, table.id);
  const followers = upgraded.columns.find(c => c.name === 'Followers')!.id;
  for (const count of ['4029', '~1万', null]) {
    await runtime.data.ingestDiscoverySources(ctx, { table_id: table.id, campaign_id: 'mouse', criteria_revision: 1, source_revision: 1, rows: [{ search_hit_id: 'hit', candidate_id: null, native_author_id: null, verdict: 'unverified', reasons: [], contacts: [], evidence_version: 'a'.repeat(64), values: { [category]: '', [followers]: count } }] });
    const rows = await runtime.data.queryRows(ctx, table.id, 1, 100);
    assert.equal(rows.rows.length, 1);
    assert.equal(rows.rows[0].values[followers] ?? null, count);
  }
  console.log('Empty optional category publishes successfully.');
} finally { await runtime.data.close(); await rm(root, { recursive: true, force: true }); }
