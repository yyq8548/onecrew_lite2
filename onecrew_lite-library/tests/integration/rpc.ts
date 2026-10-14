import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { hc, parseResponse } from 'hono/client';
import { createApp, type AppType } from '../../src/app.js';
import { TestAccess } from '../helpers/test-access.js';

const profile = await mkdtemp(path.join(os.tmpdir(), 'onecrew-rpc-'));
const runtime = await createApp(profile, new TestAccess());
const client = hc<AppType>('http://127.0.0.1:18091', {
  fetch: (input: RequestInfo | URL, init?: RequestInit) => {
    const request = new Request(input, init);
    request.headers.set('Host', new URL(request.url).host);
    return runtime.app.request(request);
  },
}).api.v1;

try {
  assert.equal((await parseResponse(client.access.status.$get())).state, 'signed_in');
  assert.equal((await parseResponse(client.browser.status.$get())).connected, false);

  const table = await parseResponse(
    client['dynamic-tables'].import.$post({
      form: {
        file: new File(['Name,Count\nAda,42\n'], 'creators.csv', { type: 'text/csv' }),
        name: 'RPC 导入 & export',
      },
    }),
  );
  assert.equal(table.name, 'RPC 导入 & export');
  assert.ok(table.web_url.includes(table.id));
  const tables = client['dynamic-tables'];
  const param = { tableId: table.id };
  const found = await parseResponse(tables.$get({ query: { page: '1', size: '20', q: '&' } }));
  assert.deepEqual(
    found.tables.map((entry) => entry.id),
    [table.id],
  );
  assert.equal((await parseResponse(tables[':tableId'].$get({ param }))).id, table.id);

  const nameColumn = table.columns.find((column) => column.name === 'Name');
  assert.ok(nameColumn);
  const rows = await parseResponse(tables[':tableId'].rows.$get({ param, query: {} }));
  assert.equal(rows.total, 1);
  const row = rows.rows[0];
  assert.ok(row);
  assert.equal(row.values[nameColumn.id], 'Ada');

  await parseResponse(
    tables[':tableId'].rows.$patch({
      param,
      json: { rows: [{ id: row.id, values: { [nameColumn.id]: 'Updated' } }] },
    }),
  );
  const renamed = await parseResponse(
    tables[':tableId'].columns[':columnId'].$patch({
      param: { ...param, columnId: nameColumn.id },
      json: { name: 'Creator' },
    }),
  );
  assert.equal(renamed.columns.find((column) => column.id === nameColumn.id)?.name, 'Creator');
  const updated = await parseResponse(tables[':tableId'].rows.$get({ param, query: {} }));
  assert.equal(updated.rows[0]?.values[nameColumn.id], 'Updated');

  const exported = await tables[':tableId'].export.$get({ param, query: { locale: 'en' } });
  assert.equal(exported.status, 200);
  assert.ok(exported.headers.get('content-disposition')?.includes('filename*='));
  assert.ok((await exported.blob()).size > 0);
  assert.equal((await tables[':tableId'].rows.$get({ param, query: { size: '0' } })).status, 400);
  await parseResponse(
    tables[':tableId'].rows[':rowId'].$delete({ param: { ...param, rowId: row.id } }),
  );
  assert.equal((await parseResponse(tables[':tableId'].rows.$get({ param, query: {} }))).total, 0);

  // RPC must still respect the same local-request and login gates as raw HTTP.
  assert.equal((await client.access.logout.$post()).status, 403);
  const signedOut = await parseResponse(
    client.access.logout.$post(undefined, {
      headers: { 'X-OneCrew-Request': '1', 'Content-Type': 'application/json' },
    }),
  );
  assert.equal(signedOut.state, 'signed_out');
  assert.equal((await tables.$get({ query: {} })).status, 401);
  console.log(
    'RPC integration passed: typed requests, multipart import, query encoding, row edits, export, validation and auth gates',
  );
} finally {
  runtime.browsers.closeAll();
  await runtime.closeMcp();
  await runtime.data.close();
  await rm(profile, { recursive: true, force: true });
}
