import { TestAccess } from '../helpers/test-access.js';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { serve } from '@hono/node-server';
import { Client } from '@modelcontextprotocol/client';
import { StdioClientTransport } from '@modelcontextprotocol/client/stdio';
import { createApp } from '../../src/app.js';

const resultOf = <T>(reply: { structuredContent?: unknown }): T =>
  (reply.structuredContent as { result: T }).result;

const temp = await mkdtemp(path.join(os.tmpdir(), 'onecrew-stdio-'));
const descriptor = path.join(temp, 'server.json');
const access = new TestAccess();
const originalUser = access.user;
const runtime = await createApp(path.join(temp, 'data'), access);
let server: ReturnType<typeof serve>;
async function start() {
  const instanceId = randomUUID();
  const port = await new Promise<number>((resolve) => {
    server = serve(
      {
        fetch: (request) =>
          new URL(request.url).pathname === '/_local/status'
            ? Response.json({ instance_id: instanceId })
            : runtime.app.fetch(request),
        hostname: '127.0.0.1',
        port: 0,
      },
      (info) => resolve(info.port),
    );
  });
  await writeFile(
    descriptor,
    JSON.stringify({
      pid: process.pid,
      port,
      url: `http://127.0.0.1:${port}`,
      service: 'onecrew_lite',
      protocol_version: 1,
      instance_id: instanceId,
      control_token: 'test-control-token',
      data_dir: path.join(temp, 'data'),
    }),
  );
}
async function stop() {
  await new Promise<void>((resolve, reject) =>
    server.close((error) => (error ? reject(error) : resolve())),
  );
}
async function run(mode: 'legacy' | { pin: string }) {
  const client = new Client(
    { name: 'bridge-test', version: '1' },
    { versionNegotiation: { mode } },
  );
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: ['--import', 'tsx', path.resolve('src/mcp/stdio.ts')],
    env: {
      ...Object.fromEntries(
        Object.entries(process.env).filter(
          (entry): entry is [string, string] => entry[1] !== undefined,
        ),
      ),
      ONECREW_LITE_RUNTIME_FILE: descriptor,
      ONECREW_LITE_DATA_DIR: path.join(temp, 'data'),
    },
    stderr: 'pipe',
  });
  try {
    await client.connect(transport);
    const tools = await client.listTools();
    assert.equal(tools.tools.length, 11);
    assert(tools.tools.some((tool) => tool.name === 'platform_search_create'));
    const result = await client.callTool({ name: 'table_list', arguments: {} });
    assert(!result.isError);
    assert(client.getInstructions(), 'a client without a plugin receives server instructions');
    const created = await client.callTool({
      name: 'table_create',
      arguments: {
        name: `Protocol workflow ${JSON.stringify(mode)}`,
        columns: [
          { name: 'Creator', data_type: 'people' },
          { name: 'Channel', data_type: 'website' },
          { name: 'Match', data_type: 'score' },
        ],
      },
    });
    assert(!created.isError);
    const table = resultOf<{
      id: string;
      web_url: string;
      columns: Array<{ id: string; data_type: string }>;
    }>(created);
    const field = (type: string) => table.columns.find((column) => column.data_type === type)!.id;
    const values = {
      [field('people')]: {
        name: 'Protocol fixture',
        image_url: 'https://example.com/avatar.png',
        identity: { platform: 'youtube', platform_id: 'UCfixture000000000000000' },
      },
      [field('website')]: 'https://www.youtube.com/@fixture',
      [field('score')]: { level: 'good', reason: 'Fixture evidence for protocol verification' },
    };
    const written = await client.callTool({
      name: 'table_create_rows',
      arguments: { table_id: table.id, rows: [{ values }] },
    });
    assert(!written.isError);
    assert.equal(resultOf<{ web_url: string }>(written).web_url, table.web_url);
    const queried = await client.callTool({
      name: 'table_query_rows',
      arguments: { table_id: table.id },
    });
    assert(!queried.isError);
    const rows = resultOf<{ rows: Array<{ values: unknown }> }>(queried).rows;
    assert.equal(rows.length, 1);
    assert.deepEqual(rows[0].values, values);
    await stop();
    await start();
    const after = await client.callTool({ name: 'table_list', arguments: {} });
    assert(!after.isError, 'existing stdio connection follows port changes');
    access.user = { id: 'other-user', email: 'other@example.test' };
    const switched = await client.callTool({ name: 'table_list', arguments: {} });
    assert(switched.isError, 'an existing bridge must not follow an account switch');
    const status = await client.callTool({ name: 'account', arguments: { action: 'status' } });
    assert(!status.isError, 'account status remains available so clients can explain reconnecting');
    access.user = originalUser;
    assert(!(await client.callTool({ name: 'table_list', arguments: {} })).isError);
  } finally {
    access.user = originalUser;
    await client.close();
  }
}
try {
  await start();
  await run('legacy');
  await run({ pin: '2026-07-28' });
  console.log(
    'stdio integration ok: forwarded schemas/calls and automatic port change in modern + legacy modes',
  );
} finally {
  await stop();
  runtime.browsers.closeAll();
  await runtime.closeMcp();
  await runtime.data.close();
  await rm(temp, { recursive: true, force: true });
}
