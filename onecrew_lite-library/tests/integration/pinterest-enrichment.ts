import assert from 'node:assert/strict';
import { createHash, randomUUID } from 'node:crypto';
import { mkdir, mkdtemp, readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createEmbeddedTables } from '../../src/embedded.js';

const receipt = await readFile(new URL('../../../onecrew_lite-app/tests/fixtures/pinterest-monthly-role-review-20261019.json', import.meta.url), 'utf8');
const { candidate } = JSON.parse(receipt);
const scratch = fileURLToPath(new URL('../../../.scratch/embedded-tables/', import.meta.url));
await mkdir(scratch, { recursive: true });
const root = await mkdtemp(path.join(scratch, 'pinterest-enrichment-'));
const tables = await createEmbeddedTables(root, 'http://127.0.0.1:34567');
const tenant = await tables.tenants.resolve({ id: randomUUID(), email: 'pinterest-enrichment@example.test' });
const original = await tables.data.ensureDiscoveryCampaignTable(tenant, { campaign_id: candidate.campaign_id, name: 'Pinterest 真实月浏览量样本', columns: [{ name: 'Followers', data_type: 'string' }] });
const followerField = original.columns.find(field => field.name === 'Followers')!.id;
const source = { search_hit_id: 'df9bc8ba-1389-43ac-b559-145c9246e8d1', candidate_id: candidate.id, native_author_id: candidate.platform_account_id, verdict: candidate.verdict, reasons: candidate.evidence.assessment.reasons, contacts: [], evidence_version: createHash('sha256').update(receipt).digest('hex') };
const batch = { table_id: original.id, campaign_id: candidate.campaign_id, criteria_revision: 1, source_revision: 1 };
await tables.data.ingestDiscoverySources(tenant, { ...batch, rows: [{ ...source, values: { [followerField]: '107' } }] });
const initial = await tables.data.queryRows(tenant, original.id, 1, 100);
const upgraded = await tables.data.ensureDiscoveryCampaignTable(tenant, { campaign_id: candidate.campaign_id, name: 'Pinterest 真实月浏览量样本', columns: [{ name: 'Followers', data_type: 'string' }, { name: 'Pinterest monthly views', data_type: 'string' }, { name: 'Pinterest scale passed by', data_type: 'string' }, { name: 'Biography status', data_type: 'string' }] });
assert.equal(upgraded.id, original.id);
const monthlyField = upgraded.columns.find(field => field.name === 'Pinterest monthly views')!.id;
const branchField = upgraded.columns.find(field => field.name === 'Pinterest scale passed by')!.id;
const bioField = upgraded.columns.find(field => field.name === 'Biography status')!.id;
const values = { [followerField]: '107', [monthlyField]: candidate.evidence.pinterest_monthly_views_raw, [branchField]: candidate.evidence.assessment.pinterest_scale.passed_by.join(','), [bioField]: candidate.evidence.profile_bio_status };
for (let repeat = 0; repeat < 2; repeat++) {
  await tables.data.ingestDiscoverySources(tenant, { ...batch, rows: [{ ...source, values }] });
  const rows = await tables.data.queryRows(tenant, original.id, 1, 100);
  assert.equal(rows.rows.length, 1);
  assert.equal(rows.rows[0]!.id, initial.rows[0]!.id);
  for (const [field, value] of Object.entries(values)) assert.equal(rows.rows[0]!.values[field], value);
}
await tables.close();
console.log('真实 Pinterest 月浏览量和入选依据在已有行中补全，证据版本不变且重复同步不新增行。');
