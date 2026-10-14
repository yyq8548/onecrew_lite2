import { spawn, type ChildProcess } from 'node:child_process';
import { createServer } from 'node:net';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { Client, StreamableHTTPClientTransport } from '@modelcontextprotocol/client';

const root = process.cwd();
const profile = await mkdtemp(path.join(os.tmpdir(), 'onecrew-http-mcp-'));
const port = Number(process.env.TEST_PORT ?? 18092);
const publicWebUrl = 'https://tables.example.test/team-a/';
const env = {
  ...process.env,
  ONECREW_LITE_RUNTIME_FILE: path.join(profile, 'runtime', 'server.json'),
  PORT: String(port),
  PUBLIC_WEB_URL: publicWebUrl,
  ONECREW_LITE_DATA_DIR: profile,
};
let child: ChildProcess | undefined;

const assertPortFree = async () =>
  new Promise<void>((resolve, reject) => {
    const probe = createServer();
    probe.once('error', (error: NodeJS.ErrnoException) => {
      probe.close();
      reject(
        new Error(
          `integration port 127.0.0.1:${port} is already occupied (${error.code ?? error.message}); choose another TEST_PORT or stop the process using that port`,
        ),
      );
    });
    probe.listen(port, '127.0.0.1', () => probe.close(() => resolve()));
  });
const waitForHealth = async () => {
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    if (child?.exitCode !== null && child?.exitCode !== undefined)
      throw new Error(`server exited before health check (code ${child.exitCode})`);
    try {
      const response = await fetch(`http://127.0.0.1:${port}/healthz`);
      if (response.ok) return response.json();
    } catch {
      /* booting */
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  throw new Error('server did not become healthy');
};
const json = async <T>(url: string, init?: RequestInit) => {
  const response = await fetch(`http://127.0.0.1:${port}${url}`, {
    headers: { 'content-type': 'application/json' },
    ...init,
  });
  const body = await response.json();
  if (!response.ok) throw new Error(`${response.status}: ${JSON.stringify(body)}`);
  return body as T;
};
const expectStatus = async (url: string, expected: number, init?: RequestInit) => {
  const response = await fetch(`http://127.0.0.1:${port}${url}`, {
    headers: { 'content-type': 'application/json' },
    ...init,
  });
  if (response.status !== expected)
    throw new Error(`expected ${expected} from ${url}, received ${response.status}`);
};
const start = async () => {
  await assertPortFree();
  child = spawn(process.execPath, ['--import', 'tsx', 'tests/helpers/test-server.ts'], {
    cwd: root,
    env,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  child.stdout?.on('data', (chunk) => process.stdout.write(`[server] ${chunk}`));
  child.stderr?.on('data', (chunk) => process.stderr.write(`[server] ${chunk}`));
};
const stop = async () => {
  if (!child) return;
  const running = child;
  running.kill('SIGTERM');
  await Promise.race([
    new Promise<void>((resolve) => running.once('exit', () => resolve())),
    new Promise<void>((resolve) => setTimeout(resolve, 5_000)),
  ]);
  if (!running.killed) running.kill('SIGKILL');
  child = undefined;
};
const waitForDown = async () => {
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) {
    try {
      await fetch(`http://127.0.0.1:${port}/healthz`);
    } catch {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error('server did not stop');
};

try {
  await start();
  await waitForHealth();
  const beforeInvalid = await json<{ tables: Array<{ id: string }> }>(
    '/api/v1/dynamic-tables?page=1&size=100',
  );
  const oldPayload = {
    name: `Old contract ${Date.now()}`,
    columns: [{ name: 'Old', type: 'text' }],
  };
  await expectStatus('/api/v1/dynamic-tables', 400, {
    method: 'POST',
    body: JSON.stringify(oldPayload),
  });
  await expectStatus('/api/v1/dynamic-tables', 400, {
    method: 'POST',
    body: JSON.stringify({
      name: `Strict ${Date.now()}`,
      columns: [{ name: 'Old', data_type: 'string', presentation: {} }],
    }),
  });
  const afterInvalid = await json<{ tables: Array<{ id: string }> }>(
    '/api/v1/dynamic-tables?page=1&size=100',
  );
  if (afterInvalid.tables.length !== beforeInvalid.tables.length)
    throw new Error('old/strict table_create payload left a table behind');

  const name = `Semantic Integration ${Date.now()}`;
  const table = await json<{
    id: string;
    web_url: string;
    columns: Array<{ id: string; name: string; data_type: string }>;
  }>('/api/v1/dynamic-tables', {
    method: 'POST',
    body: JSON.stringify({
      name,
      columns: [
        { name: 'Title', data_type: 'string' },
        { name: 'Count', data_type: 'int' },
        { name: 'Fraction', data_type: 'float' },
        { name: 'Active', data_type: 'bool' },
        { name: 'When', data_type: 'date' },
        { name: 'Owner', data_type: 'people' },
        { name: 'Company', data_type: 'company' },
        { name: 'Site', data_type: 'website' },
        { name: 'Email', data_type: 'email' },
        { name: 'Phone', data_type: 'telephone' },
        { name: 'Score', data_type: 'score' },
      ],
    }),
  });
  if (table.web_url !== `${publicWebUrl.replace(/\/+$/, '')}/library/${table.id}`)
    throw new Error(`table web_url did not preserve configured prefix: ${table.web_url}`);
  if (table.columns.length !== 11 || table.columns.some((column) => !column.data_type))
    throw new Error('semantic table schema was not returned');
  const byName = new Map(table.columns.map((column) => [column.name, column]));
  const id = (name: string) => byName.get(name)!.id;
  const first = id('Title');
  const count = id('Count');
  const owner = id('Owner');
  const score = id('Score');
  await expectStatus(`/api/v1/dynamic-tables/${table.id}/columns/${first}/presentation`, 404, {
    method: 'PATCH',
    body: JSON.stringify({ presentation: { kind: 'platform-link' } }),
  });

  await expectStatus(`/api/v1/dynamic-tables/${table.id}/rows`, 400, {
    method: 'POST',
    body: JSON.stringify({ rows: [{ values: { [count]: 1.5 } }] }),
  });
  await expectStatus(`/api/v1/dynamic-tables/${table.id}/rows`, 400, {
    method: 'POST',
    body: JSON.stringify({ rows: [{ values: { [owner]: { name: '' } } }] }),
  });
  await expectStatus(`/api/v1/dynamic-tables/${table.id}/rows`, 400, {
    method: 'POST',
    body: JSON.stringify({
      rows: [{ values: { [owner]: { name: 'Ada Lovelace', identity: { platform: 'youtube' } } } }],
    }),
  });
  await expectStatus(`/api/v1/dynamic-tables/${table.id}/rows`, 400, {
    method: 'POST',
    body: JSON.stringify({
      rows: [
        {
          values: {
            [id('Company')]: {
              name: 'Analytical Engines',
              identity: { platform: '', platform_id: 'company-1' },
            },
          },
        },
      ],
    }),
  });
  await expectStatus(`/api/v1/dynamic-tables/${table.id}/rows`, 400, {
    method: 'POST',
    body: JSON.stringify({ rows: [{ values: { [score]: 80 } }] }),
  });
  await expectStatus(`/api/v1/dynamic-tables/${table.id}/rows`, 400, {
    method: 'POST',
    body: JSON.stringify({ rows: [{ values: { [id('Site')]: 'ftp://bad.example' } }] }),
  });
  await expectStatus(`/api/v1/dynamic-tables/${table.id}/rows`, 400, {
    method: 'POST',
    body: JSON.stringify({ rows: [{ values: { [id('When')]: '2024-02-30' } }] }),
  });

  const partialColumn = `Should not persist ${Date.now()}`;
  await expectStatus(`/api/v1/dynamic-tables/${table.id}/columns`, 400, {
    method: 'POST',
    body: JSON.stringify({
      columns: [
        { name: `Valid before failure ${Date.now()}`, data_type: 'string' },
        { name: partialColumn, type: 'text' },
      ],
    }),
  });
  const afterInvalidColumns = await json<{ columns: Array<{ name: string }> }>(
    `/api/v1/dynamic-tables/${table.id}`,
  );
  if (
    afterInvalidColumns.columns.some(
      (column) => column.name === partialColumn || column.name.startsWith('Valid before failure '),
    )
  )
    throw new Error('invalid table_add_columns left a partial batch');
  const added = await json<{
    id: string;
    web_url: string;
    columns: Array<{ id: string; name: string; data_type: string }>;
  }>(`/api/v1/dynamic-tables/${table.id}/columns`, {
    method: 'POST',
    body: JSON.stringify({
      columns: [
        { name: 'Nullable note', data_type: 'string' },
        { name: 'Backup person', data_type: 'people' },
      ],
    }),
  });
  if (added.web_url !== table.web_url) throw new Error('table_add_columns web_url mismatch');
  if (
    !added.columns.some(
      (column) => column.name === 'Backup person' && column.data_type === 'people',
    )
  )
    throw new Error('valid semantic add columns failed');

  const values = {
    [first]: 'persisted',
    [count]: 7,
    [id('Fraction')]: 2.5,
    [id('Active')]: true,
    [id('When')]: '2026-09-22T12:00:00+08:00',
    [owner]: {
      name: 'Ada Lovelace',
      image_url: 'https://example.com/ada.png',
      url: 'https://example.com/ada',
      identity: { platform: 'youtube', platform_id: 'ada-yt-123' },
    },
    [id('Company')]: {
      name: 'Analytical Engines',
      url: 'https://example.com/company',
      identity: { platform: 'linkedin', platform_id: 'analytical-engines' },
    },
    [id('Site')]: 'https://example.com',
    [id('Email')]: 'ada@example.com',
    [id('Phone')]: '+852 1234 5678',
    [score]: { level: 'excellent', reason: 'Strong result' },
  };
  const created = await json<{
    table_id: string;
    web_url: string;
    rows: Array<{ id: string; values: Record<string, unknown> }>;
  }>(`/api/v1/dynamic-tables/${table.id}/rows`, {
    method: 'POST',
    body: JSON.stringify({ rows: [{ values }] }),
  });
  if (
    created.table_id !== table.id ||
    created.web_url !== table.web_url ||
    created.rows.length !== 1
  )
    throw new Error('HTTP row create link/count response was invalid');
  if (
    created.rows[0]?.values[owner] &&
    (created.rows[0].values[owner] as { name: string }).name !== 'Ada Lovelace'
  )
    throw new Error('people value was not decoded');
  if (
    (created.rows[0]?.values[owner] as { identity?: { platform_id?: string } })?.identity
      ?.platform_id !== 'ada-yt-123' ||
    (created.rows[0]?.values[id('Company')] as { identity?: { platform_id?: string } })?.identity
      ?.platform_id !== 'analytical-engines'
  )
    throw new Error('people/company identity was not decoded');
  await expectStatus(
    `/api/v1/dynamic-tables/${table.id}/rows?sort_field=${owner}&sort_order=asc`,
    400,
  );
  const rowId = created.rows[0]!.id;
  const updated = await json<{ table_id: string; web_url: string; rows: Array<{ id: string }> }>(
    `/api/v1/dynamic-tables/${table.id}/rows`,
    {
      method: 'PATCH',
      body: JSON.stringify({
        rows: [{ id: rowId, values: { [score]: { level: 'good', reason: 'Updated' } } }],
      }),
    },
  );
  if (
    updated.table_id !== table.id ||
    updated.web_url !== table.web_url ||
    updated.rows.length !== 1
  )
    throw new Error('HTTP row update link/count response was invalid');
  const page = await json<{ total: number; rows: Array<{ values: Record<string, unknown> }> }>(
    `/api/v1/dynamic-tables/${table.id}/rows?size=10`,
  );
  if (page.total !== 1 || (page.rows[0]?.values[score] as { level?: string })?.level !== 'good')
    throw new Error('structured row update failed');

  await stop();
  await waitForDown();
  await start();
  await waitForHealth();
  const persisted = await json<{ total: number; rows: Array<{ values: Record<string, unknown> }> }>(
    `/api/v1/dynamic-tables/${table.id}/rows?page=1&size=10`,
  );
  if (
    persisted.total !== 1 ||
    (persisted.rows[0]?.values[owner] as { name?: string; identity?: { platform_id?: string } })
      ?.name !== 'Ada Lovelace' ||
    (persisted.rows[0]?.values[owner] as { identity?: { platform_id?: string } })?.identity
      ?.platform_id !== 'ada-yt-123' ||
    (persisted.rows[0]?.values[id('Company')] as { identity?: { platform_id?: string } })?.identity
      ?.platform_id !== 'analytical-engines'
  )
    throw new Error('structured PGlite persistence failed');
  const detail = await fetch(`http://127.0.0.1:${port}/library/${table.id}`);
  if (!detail.ok || !(await detail.text()).includes('<!doctype html>'))
    throw new Error('local table detail page was not served');

  const runMcpClient = async (
    label: string,
    versionNegotiation: { mode: 'legacy' | { pin: string } },
  ) => {
    const client = new Client(
      { name: `onecrew-${label}-client`, version: '0.1.0' },
      { versionNegotiation },
    );
    await client.connect(
      new StreamableHTTPClientTransport(new URL(`http://127.0.0.1:${port}/mcp`)),
    );
    const listed = await client.listTools();
    if (listed.tools.length !== 8)
      throw new Error(`MCP ${label} expected 8 tools, received ${listed.tools.length}`);
    if (listed.tools.some(tool => tool.name.startsWith('platform_search_')))
      throw new Error('Retired independent searches must not be exposed');
    const fetched = await client.callTool({ name: 'table_get', arguments: { table_id: table.id } });
    const fetchedTable = (
      fetched.structuredContent as { result?: { id?: string; web_url?: string } } | undefined
    )?.result;
    if (fetched.isError || fetchedTable?.id !== table.id || fetchedTable.web_url !== table.web_url)
      throw new Error(`MCP ${label} table_get web_url response was invalid`);
    const result = await client.callTool({
      name: 'table_create_rows',
      arguments: {
        table_id: table.id,
        rows: [
          {
            values: {
              [first]: `from ${label}`,
              [owner]: { name: 'MCP person' },
              [score]: { level: 'medium' },
            },
          },
        ],
      },
    });
    if (result.isError) throw new Error(`MCP ${label} structured write failed`);
    const written = (
      result.structuredContent as
        { result?: { table_id?: string; web_url?: string; rows?: unknown[] } } | undefined
    )?.result;
    if (
      written?.table_id !== table.id ||
      written.web_url !== table.web_url ||
      written.rows?.length !== 1
    )
      throw new Error(`MCP ${label} structured link/count response was invalid`);
    await client.close();
    return listed;
  };
  const modernTools = await runMcpClient('modern', { mode: { pin: '2026-07-28' } });
  const legacyTools = await runMcpClient('legacy', { mode: 'legacy' });
  if (legacyTools.tools.length !== modernTools.tools.length)
    throw new Error('MCP modern and legacy tool lists differ');
  const afterMcp = await json<{ total: number }>(
    `/api/v1/dynamic-tables/${table.id}/rows?page=1&size=10`,
  );
  if (afterMcp.total !== 3) throw new Error('MCP structured writes were not persisted');
  console.log(
    `integration ok: strict semantic columns/values, atomic validation, restart persistence, and MCP modern/legacy (${modernTools.tools.map((tool) => tool.name).join(', ')})`,
  );
} finally {
  await stop();
  await rm(profile, { recursive: true, force: true });
}
