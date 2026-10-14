import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { serve } from '@hono/node-server';
import { Client, StreamableHTTPClientTransport } from '@modelcontextprotocol/client';
import { sql } from 'kysely';
import { createApp } from '../../src/app.js';
import { AccessError } from '../../src/modules/access/client.js';
import { TestAccess } from '../helpers/test-access.js';

const alice = { id: 'tenant-test-alice', email: 'alice@example.test' };
const bob = { id: 'tenant-test-bob', email: 'bob@example.test' };
const users = new Map([
  ['Bearer alice', alice],
  ['Bearer bob', bob],
]);
// This resolver is injected by the test, never selectable through production env/headers.
const principal = async (request: Request) => {
  const user = users.get(request.headers.get('authorization') ?? '');
  if (!user) throw new AccessError('access.login_required', 401);
  return user;
};
type Table = { id: string; name: string; columns: Array<{ id: string; name: string }> };
type Rows = { rows: Array<{ id: string; values: Record<string, unknown> }> };
const root = await mkdtemp(path.join(os.tmpdir(), 'onecrew-tenancy-'));
const runtime = await createApp(root, new TestAccess(alice), principal);
const request = async (user: 'alice' | 'bob', url: string, init: RequestInit = {}) => {
  const headers = new Headers(init.headers);
  headers.set('authorization', `Bearer ${user}`);
  headers.set('host', '127.0.0.1:8091');
  headers.set('origin', 'http://127.0.0.1:8091');
  if (typeof init.body === 'string') headers.set('content-type', 'application/json');
  return runtime.app.request(`http://127.0.0.1:8091${url}`, { ...init, headers });
};
const json = async <T>(response: Promise<Response>): Promise<T> => {
  const result = await response;
  assert(result.ok, `${result.status}: ${await result.clone().text()}`);
  return result.json() as Promise<T>;
};
const create = (user: 'alice' | 'bob', name: string) =>
  json<Table>(
    request(user, '/api/v1/dynamic-tables', {
      method: 'POST',
      body: JSON.stringify({ name, columns: [{ name: 'Name', data_type: 'string' }] }),
    }),
  );
let server: ReturnType<typeof serve> | undefined;
const clients: Client[] = [];
try {
  // Concurrent principals use independent contexts even though they share one process/database.
  const [tableA, tableB] = await Promise.all([create('alice', 'Alice'), create('bob', 'Bob')]);
  const [contextA, contextB] = await Promise.all([
    runtime.tenants.resolve(alice),
    runtime.tenants.resolve(bob),
  ]);
  assert.notEqual(contextA.tenantId, contextB.tenantId);
  assert.notEqual(contextA.baseId, contextB.baseId);
  const again = await Promise.all(Array.from({ length: 5 }, () => runtime.tenants.resolve(alice)));
  assert(again.every((context) => context.tenantId === contextA.tenantId));
  const lists = await Promise.all(
    (['alice', 'bob'] as const).map((user) =>
      json<{ tables: Table[] }>(request(user, '/api/v1/dynamic-tables')),
    ),
  );
  assert.deepEqual(
    lists[0].tables.map((table) => table.id),
    [tableA.id],
  );
  assert.deepEqual(
    lists[1].tables.map((table) => table.id),
    [tableB.id],
  );
  const rowsA = await json<Rows>(
    request('alice', `/api/v1/dynamic-tables/${tableA.id}/rows`, {
      method: 'POST',
      body: JSON.stringify({ rows: [{ values: { [tableA.columns[0].id]: 'private A' } }] }),
    }),
  );
  const rowsB = await json<Rows>(
    request('bob', `/api/v1/dynamic-tables/${tableB.id}/rows`, {
      method: 'POST',
      body: JSON.stringify({ rows: [{ values: { [tableB.columns[0].id]: 'private B' } }] }),
    }),
  );
  const foreign = `/api/v1/dynamic-tables/${tableA.id}`;
  const attempts: Array<[string, RequestInit]> = [
    [foreign, {}],
    [`${foreign}/rows`, {}],
    [`${foreign}/export`, {}],
    [
      `${foreign}/columns`,
      { method: 'POST', body: JSON.stringify({ columns: [{ name: 'Bad', data_type: 'string' }] }) },
    ],
    [
      `${foreign}/columns/${tableA.columns[0].id}`,
      { method: 'PATCH', body: JSON.stringify({ name: 'Bad' }) },
    ],
    [
      `${foreign}/rows`,
      {
        method: 'POST',
        body: JSON.stringify({ rows: [{ values: { [tableA.columns[0].id]: 'Bad' } }] }),
      },
    ],
    [
      `${foreign}/rows`,
      {
        method: 'PATCH',
        body: JSON.stringify({
          rows: [{ id: rowsA.rows[0].id, values: { [tableA.columns[0].id]: 'Bad' } }],
        }),
      },
    ],
    [`${foreign}/rows/${rowsA.rows[0].id}`, { method: 'DELETE' }],
  ];
  for (const [url, init] of attempts) {
    const denied = await request('bob', url, init);
    assert.equal(denied.status, 404, `${init.method ?? 'GET'} ${url}: ${await denied.text()}`);
  }
  // A valid owned table does not authorize IDs from another table.
  const badColumn = await request('alice', `${foreign}/columns/${tableB.columns[0].id}`, {
    method: 'PATCH',
    body: JSON.stringify({ name: 'Bad' }),
  });
  assert(badColumn.status >= 400 && badColumn.status < 500);
  const badRow = await request('alice', `${foreign}/rows/${rowsB.rows[0].id}`, {
    method: 'DELETE',
  });
  assert.equal(badRow.status, 404);
  const badUpdate = await request('alice', `${foreign}/rows`, {
    method: 'PATCH',
    body: JSON.stringify({
      rows: [{ id: rowsB.rows[0].id, values: { [tableA.columns[0].id]: 'Bad' } }],
    }),
  });
  assert(badUpdate.status >= 400 && badUpdate.status < 500);
  const before = await json<Rows>(request('alice', `${foreign}/rows`));
  assert.deepEqual(
    before.rows.map((row) => row.values),
    [{ [tableA.columns[0].id]: 'private A' }],
  );
  const stale = await request('bob', '/api/v1/dynamic-tables', {
    headers: { 'X-OneCrew-Account-Id': alice.id },
  });
  assert.equal(stale.status, 409);

  // Imports are created in the authenticated space, never in a global default base.
  const form = new FormData();
  form.set('file', new File(['Name,Count\nImported B,1\n'], 'import.csv'));
  const imported = await json<Table>(
    request('bob', '/api/v1/dynamic-tables/import', { method: 'POST', body: form }),
  );
  assert.equal((await request('alice', `/api/v1/dynamic-tables/${imported.id}`)).status, 404);

  // MCP uses the same per-request principal resolver as HTTP.
  server = serve({ fetch: runtime.app.fetch, hostname: '127.0.0.1', port: 0 });
  if (!server.listening) await new Promise<void>((resolve) => server!.once('listening', resolve));
  const port = (server.address() as { port: number }).port;
  for (const user of ['alice', 'bob'] as const) {
    const client = new Client({ name: `tenancy-${user}`, version: '1' });
    clients.push(client);
    await client.connect(
      new StreamableHTTPClientTransport(new URL(`http://127.0.0.1:${port}/mcp`), {
        requestInit: { headers: { Authorization: `Bearer ${user}` } },
      }),
    );
  }
  assert(
    !(await clients[0].callTool({ name: 'table_get', arguments: { table_id: tableA.id } })).isError,
  );
  assert(
    (await clients[1].callTool({ name: 'table_get', arguments: { table_id: tableA.id } })).isError,
  );
  assert(
    (await clients[1].callTool({ name: 'table_query_rows', arguments: { table_id: tableA.id } }))
      .isError,
  );
  assert(
    (
      await clients[1].callTool({
        name: 'table_create_rows',
        arguments: { table_id: tableA.id, rows: [{ values: { [tableA.columns[0].id]: 'Bad' } }] },
      })
    ).isError,
  );

  const bridgeA = runtime.browsers.get(contextA);
  const bridgeB = runtime.browsers.get(contextB);
  const extensionId = 'a'.repeat(32);
  const connectionA = bridgeA.connect(extensionId);
  assert.equal(bridgeB.status().connected, false);
  assert.throws(() => bridgeB.authenticate(connectionA.token, extensionId));
  bridgeB.connect(extensionId);
  runtime.browsers.closeForUser(alice.id);
  assert.equal(bridgeA.status().connected, false);
  assert.equal(bridgeB.status().connected, true, 'logout must not disconnect another tenant');
  assert.equal((await request('bob', '/api/v1/searches/retired')).status, 404);
  for (const client of clients) {
    const catalog = await client.listTools();
    assert(!catalog.tools.some(tool => tool.name.startsWith('platform_search_')));
  }
  const foreignKeys = await sql<{ table_name: string; target: string }>`
    SELECT t.relname AS table_name, parent.relname AS target
    FROM pg_constraint c JOIN pg_class t ON t.oid = c.conrelid
    JOIN pg_class parent ON parent.oid = c.confrelid
    WHERE c.contype = 'f' AND t.relname LIKE 'onecrew_%'
    ORDER BY t.relname, parent.relname`.execute(runtime.database.db);
  // 002/003 add only the explicit projection ownership chain, never foreign keys
  // into Teable-managed metadata or user tables.
  assert.deepEqual(foreignKeys.rows, [
    { table_name: 'onecrew_discovery_fields', target: 'onecrew_discovery_tables' },
    { table_name: 'onecrew_discovery_identity_snapshots', target: 'onecrew_discovery_tables' },
    { table_name: 'onecrew_discovery_rows', target: 'onecrew_discovery_tables' },
    { table_name: 'onecrew_discovery_tables', target: 'onecrew_tenants' },
  ]);
  console.log(
    'Tenant isolation passed: concurrent principals, table/field/row/export/import guards, HTTP + MCP, stale account header and retired search tools.',
  );
} finally {
  await Promise.all(clients.map((client) => client.close()));
  if (server) await new Promise<void>((resolve) => server!.close(() => resolve()));
  runtime.browsers.closeAll();
  await runtime.closeMcp();
  await runtime.data.close();
  await rm(root, { recursive: true, force: true });
}
