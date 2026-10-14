import assert from 'node:assert/strict';
import { randomBytes } from 'node:crypto';
import { once } from 'node:events';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { serve } from '@hono/node-server';
import { createApp } from '../../src/app.js';
import { createDiscoverySyncRoutes } from '../../src/modules/tables/discovery-sync-http.js';
import { TestAccess } from '../helpers/test-access.js';

// Isolated receiver acceptance. Synthetic producer rows deliberately do not
// claim live platform evidence or a full Rust producer/UI acceptance.
const root = await mkdtemp(path.join(os.tmpdir(), 'onecrew-score-http-'));
const access = new TestAccess({ id: 'score-http-test', email: 'score@example.test' });
const runtime = await createApp(root, access);
const token = randomBytes(32).toString('hex');
const routes = createDiscoverySyncRoutes(runtime.data, runtime.resolveTenant, token);
const producer = serve({ fetch: routes.fetch, hostname: '127.0.0.1', port: 0 });
const web = serve({ fetch: runtime.app.fetch, hostname: '127.0.0.1', port: 0 });
try {
  if (!producer.listening) await once(producer, 'listening');
  if (!web.listening) await once(web, 'listening');
  const producerAddress = producer.address();
  const webAddress = web.address();
  assert(producerAddress && typeof producerAddress !== 'string');
  assert(webAddress && typeof webAddress !== 'string');
  const producerUrl = `http://127.0.0.1:${producerAddress.port}`;
  const webUrl = `http://127.0.0.1:${webAddress.port}`;
  const headers = {
    Authorization: `Bearer ${token}`,
    'X-OneCrew-Account-Id': access.user.id,
    'Content-Type': 'application/json',
  };
  const post = async (endpoint: string, body: unknown) => {
    const response = await fetch(`${producerUrl}/_local/discovery/${endpoint}`, {
      method: 'POST',
      headers,
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(10000),
    });
    assert.equal(response.status, 200, await response.clone().text());
    return response.json();
  };
  const campaign = 'score-http-acceptance';
  const table = (await post('campaign', {
    campaign_id: campaign,
    name: '语言验收_1005_1',
    columns: [{ name: 'Match evaluation', data_type: 'score' }],
  })) as {
    id: string;
    columns: Array<{ id: string; name: string }>;
  };
  const score = table.columns.find((column) => column.name === 'Match evaluation')!.id;
  const base = { table_id: table.id, campaign_id: campaign, criteria_revision: 1 };
  const row = {
    search_hit_id: 'language-hit',
    candidate_id: 'language-candidate',
    native_author_id: 'native-author',
    verdict: 'pending',
    reasons: ['language_unverified'],
    evidence_version: 'a'.repeat(64),
    values: { [score]: null as unknown },
  };
  const initial = await post('ingest', { ...base, source_revision: 1, rows: [row] });
  assert.equal(initial.inserted_rows, 1);
  const rowId = initial.rows[0].row_id;
  const readRows = async () => {
    const response = await fetch(`${webUrl}/api/v1/dynamic-tables/${table.id}/rows`, {
      headers: { Origin: webUrl, 'X-OneCrew-Account-Id': access.user.id },
      signal: AbortSignal.timeout(10000),
    });
    assert.equal(response.status, 200, await response.clone().text());
    return response.json();
  };
  for (const [index, language] of ['英文', '日文'].entries()) {
    const rating = {
      level: 'good',
      reason: `粉丝评论语言符合：${language}；博主本人内容语言未核实。`,
    };
    const batch = {
      ...base,
      source_revision: index + 2,
      rows: [
        {
          ...row,
          verdict: 'qualified',
          reasons: ['initial_match_good'],
          evidence_version: String(index + 2).repeat(64),
          values: { [score]: rating },
        },
      ],
    };
    assert.equal((await post('ingest', batch)).updated_rows, 1);
    assert.equal((await post('ingest', batch)).inserted_rows, 0);
    const read = await readRows();
    assert.equal(read.total, 1);
    assert.equal(read.rows[0].id, rowId);
    assert.deepEqual(read.rows[0].values[score], rating);
  }
  // A human override qualifies without inventing an AI score or hiding unknown language.
  const manual = {
    ...base,
    source_revision: 4,
    rows: [
      {
        ...row,
        verdict: 'qualified',
        reasons: ['manual_approved', 'language_unverified'],
        evidence_version: 'd'.repeat(64),
      },
    ],
  };
  assert.equal((await post('ingest', manual)).updated_rows, 1);
  assert.equal((await post('ingest', manual)).updated_rows, 0);
  const finalRows = await readRows();
  assert.equal(finalRows.total, 1);
  assert.equal(finalRows.rows[0].id, rowId);
  assert.equal(finalRows.rows[0].values[score], null);
  const stateResponse = await fetch(
    `${producerUrl}/_local/discovery/state?table_id=${table.id}&campaign_id=${campaign}`,
    { headers },
  );
  assert.equal(stateResponse.status, 200);
  const state = await stateResponse.json();
  assert.equal(state.rows[0].verdict, 'qualified');
  assert.deepEqual(state.rows[0].reasons, ['manual_approved', 'language_unverified']);
  console.log(
    'Discovery score HTTP: audience Good rationale, human override and idempotent row readback PASS',
  );
} finally {
  await Promise.all(
    [producer, web].map(
      (server) =>
        new Promise<void>((resolve, reject) => {
          server.close((error) => (error ? reject(error) : resolve()));
          if ('closeAllConnections' in server) server.closeAllConnections();
        }),
    ),
  );
  await runtime.closeMcp();
  runtime.browsers.closeAll();
  await runtime.data.close();
  await rm(root, { recursive: true, force: true });
}
