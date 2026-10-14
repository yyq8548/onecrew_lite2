import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { sql } from 'kysely';
import { createApp } from '../../src/app.js';
import { recoverInterruptedSearches } from '../../src/modules/search/legacy-recovery.js';
import { TestAccess } from '../helpers/test-access.js';

const root = await mkdtemp(path.join(os.tmpdir(), 'onecrew-lite-retired-search-'));
const runtime = await createApp(root, new TestAccess());
try {
  for (const [route, method] of [
    ['/api/v1/searches', 'POST'],
    ['/api/v1/searches/old-job', 'GET'],
    ['/api/v1/searches/old-job/results', 'GET'],
  ]) {
    const response = await runtime.app.request(`http://127.0.0.1:8191${route}`, {
      method,
      headers: { Host: '127.0.0.1:8191', Origin: 'http://127.0.0.1:8191' },
    });
    assert.equal(response.status, 404, `${method} ${route} must stay retired`);
  }
  for (const status of ['queued', 'running', 'completed', 'failed']) {
    await sql`INSERT INTO onecrew_search_jobs
      (id, tenant_id, platform, query, "limit", status, error, result_json, created_at, updated_at)
      VALUES (${status}, 'legacy-tenant', 'youtube', 'retained history', 10,
        ${status}, NULL, '{"channels":[]}', '2026-01-01', '2026-01-01')`
      .execute(runtime.database.db);
  }
  await recoverInterruptedSearches(runtime.database.db);
  const rows = await sql<{ id: string; status: string; error: string | null; result_json: string }>`
    SELECT id, status, error, result_json FROM onecrew_search_jobs ORDER BY id`
    .execute(runtime.database.db);
  assert.equal(rows.rows.length, 4, 'retiring search must preserve history');
  for (const row of rows.rows) {
    assert.equal(row.result_json, '{"channels":[]}');
    if (row.id === 'queued' || row.id === 'running') {
      assert.equal(row.status, 'failed');
      assert.equal(row.error, 'Search interrupted by server restart');
    } else {
      assert.equal(row.status, row.id);
      assert.equal(row.error, null);
    }
  }
  await recoverInterruptedSearches(runtime.database.db);
  const again = await sql`SELECT id, status, error, result_json FROM onecrew_search_jobs ORDER BY id`
    .execute(runtime.database.db);
  assert.deepEqual(again.rows, rows.rows, 'recovery must be idempotent');
  console.log('Retired search routes and preserved legacy recovery passed.');
} finally {
  runtime.browsers.closeAll();
  await runtime.closeMcp();
  await runtime.data.close();
  await rm(root, { recursive: true, force: true });
}
