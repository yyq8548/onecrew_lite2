import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { serve } from '@hono/node-server';
import { Client, StreamableHTTPClientTransport } from '@modelcontextprotocol/client';
import { sql } from 'kysely';
import { createApp } from '../../src/app.js';
import { TableDataError } from '../../src/modules/tables/data.js';
import { TestAccess } from '../helpers/test-access.js';

const alice = { id: 'projection-alice', email: 'alice@example.test' };
const bob = { id: 'projection-bob', email: 'bob@example.test' };
const root = await mkdtemp(path.join(os.tmpdir(), 'onecrew-field-protection-'));
let runtime = await createApp(root, new TestAccess(alice));
let server: ReturnType<typeof serve> | undefined;
const client = new Client({ name: 'projection-protection-test', version: '1' });
const readOnlyError = (error: unknown) =>
  error instanceof TableDataError &&
  error.status === 409 &&
  error.code === 'data.system_field_read_only';
try {
  const ctx = await runtime.tenants.resolve(alice);
  const foreignCtx = await runtime.tenants.resolve(bob);
  const table = await runtime.data.createTable(ctx, 'System and manual', [
    { name: 'Qualification', data_type: 'string' },
    { name: 'Notes', data_type: 'string' },
  ]);
  const [system, manual] = table.columns;
  const [row] = await runtime.data.createRows(ctx, table.id, [
    { values: { [system.id]: 'pending', [manual.id]: 'keep this note' } },
  ]);
  assert.equal(system.read_only, undefined, 'a name alone never claims a system field');
  // Only this test seeds trusted ownership directly; ordinary APIs cannot claim a field.
  await sql`INSERT INTO onecrew_discovery_tables
    (table_id, tenant_id, campaign_id, projection_format, created_at)
    VALUES (${table.id}, ${ctx.tenantId}, 'test-campaign', 1, '2026-09-27')`.execute(
    runtime.database.db,
  );
  await sql`INSERT INTO onecrew_discovery_fields (table_id, field_id, role)
    VALUES (${table.id}, ${system.id}, 'qualification')`.execute(runtime.database.db);
  const marked = await runtime.data.getTable(ctx, table.id);
  assert.equal(marked.columns[0].read_only, true);
  assert.equal(marked.columns[1].read_only, undefined);
  assert.equal((await runtime.data.listTables(ctx, 1, 20)).tables[0].columns[0].read_only, true);
  for (const value of ['qualified', null, 'pending']) {
    await assert.rejects(
      runtime.data.updateRows(ctx, table.id, [{ id: row.id, values: { [system.id]: value } }]),
      readOnlyError,
    );
    await assert.rejects(
      runtime.data.createRows(ctx, table.id, [{ values: { [system.id]: value } }]),
      readOnlyError,
    );
  }
  await assert.rejects(
    runtime.data.renameColumn(ctx, table.id, system.id, 'Manual status'),
    readOnlyError,
  );
  await assert.rejects(
    runtime.data.addColumns(ctx, table.id, [{ name: system.name, data_type: 'score' }]),
    readOnlyError,
  );
  assert.equal((await runtime.data.getTable(ctx, table.id)).columns[0].data_type, 'string');
  await assert.rejects(
    runtime.data.updateRows(ctx, table.id, [
      { id: row.id, values: { [manual.id]: 'must not partially write' } },
      { id: row.id, values: { [system.id]: 'qualified' } },
    ]),
    readOnlyError,
  );
  await assert.rejects(
    runtime.data.createRows(ctx, table.id, [
      { values: { [manual.id]: 'must not partially insert' } },
      { values: { [system.id]: 'qualified' } },
    ]),
    readOnlyError,
  );
  const unchanged = await runtime.data.queryRows(ctx, table.id, 1, 20);
  assert.equal(unchanged.rows.length, 1);
  assert.deepEqual(unchanged.rows[0].values, row.values);
  await assert.rejects(runtime.data.getTable(foreignCtx, table.id), { status: 404 });
  await assert.rejects(
    runtime.data.updateRows(foreignCtx, table.id, [{ id: row.id, values: { [system.id]: null } }]),
    { status: 404 },
  );
  // HTTP and MCP go through the same owner guard, rather than relying on disabled UI.
  const response = await runtime.app.request(
    `http://127.0.0.1:8091/api/v1/dynamic-tables/${table.id}/rows`,
    {
      method: 'PATCH',
      headers: { 'content-type': 'application/json', origin: 'http://127.0.0.1:8091' },
      body: JSON.stringify({ rows: [{ id: row.id, values: { [system.id]: 'qualified' } }] }),
    },
  );
  assert.equal(response.status, 409, await response.clone().text());
  assert.equal((await response.json()).error.code, 'data.system_field_read_only');
  server = serve({ fetch: runtime.app.fetch, hostname: '127.0.0.1', port: 0 });
  if (!server.listening) await new Promise<void>((resolve) => server!.once('listening', resolve));
  const port = (server.address() as { port: number }).port;
  await client.connect(new StreamableHTTPClientTransport(new URL(`http://127.0.0.1:${port}/mcp`)));
  const denied = await client.callTool({
    name: 'table_update_rows',
    arguments: { table_id: table.id, rows: [{ id: row.id, values: { [system.id]: 'qualified' } }] },
  });
  assert.equal(denied.isError, true);
  assert.match(JSON.stringify(denied), /Discovery system fields are read-only/);
  await runtime.data.renameColumn(ctx, table.id, manual.id, 'Qualification notes');
  await runtime.data.updateRows(ctx, table.id, [{ id: row.id, values: { [manual.id]: 'edited' } }]);
  await runtime.data.createRows(ctx, table.id, [{ values: { [manual.id]: 'manual row' } }]);
  await client.close();
  await runtime.closeMcp();
  await new Promise<void>((resolve, reject) => server!.close((e) => (e ? reject(e) : resolve())));
  server = undefined;
  await runtime.data.close();
  runtime = await createApp(root, new TestAccess(alice));
  const reopened = await runtime.tenants.resolve(alice);
  assert.equal((await runtime.data.getTable(reopened, table.id)).columns[0].read_only, true);
  await assert.rejects(
    runtime.data.renameColumn(reopened, table.id, system.id, 'Renamed'),
    readOnlyError,
  );
  const persisted = await runtime.data.queryRows(reopened, table.id, 1, 20);
  assert.equal(persisted.rows.length, 2);
  assert.equal(persisted.rows.find((r) => r.id === row.id)?.values[manual.id], 'edited');
  assert.equal(persisted.rows.find((r) => r.id === row.id)?.values[system.id], 'pending');
  console.log('Discovery field protection passed: owner, HTTP, MCP, manual edits, restart.');
} finally {
  await client.close();
  await runtime.closeMcp();
  if (server) await new Promise<void>((resolve) => server!.close(() => resolve()));
  await runtime.data.close();
  await rm(root, { recursive: true, force: true });
}
