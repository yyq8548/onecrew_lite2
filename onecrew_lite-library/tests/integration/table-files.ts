import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import * as XLSX from 'xlsx';
import { serve } from '@hono/node-server';
import { Client, StreamableHTTPClientTransport } from '@modelcontextprotocol/client';
import { createApp } from '../../src/app.js';
import { TestAccess } from '../helpers/test-access.js';

const root = await mkdtemp(path.join(os.tmpdir(), 'onecrew-table-files-'));
process.env.PUBLIC_WEB_URL = 'https://tables.example.test';
let runtime: Awaited<ReturnType<typeof createApp>> | undefined;
const totalTimeout = setTimeout(() => {
  console.error('table files integration exceeded its 5 minute deadline');
  const forceExit = setTimeout(() => process.exit(1), 10_000);
  void (async () => {
    await runtime?.closeMcp().catch(() => undefined);
    await runtime?.data.close().catch(() => undefined);
    await rm(root, { recursive: true, force: true });
    clearTimeout(forceExit);
    process.exit(1);
  })();
}, 5 * 60_000);
const request = async (url: string, init: RequestInit = {}) => {
  let timer: NodeJS.Timeout | undefined;
  try {
    return await Promise.race([
      runtime!.app.request(`http://127.0.0.1${url}`, init),
      new Promise<never>((_, reject) => {
        timer = setTimeout(
          () => reject(new Error(`table files request timed out: ${url}`)),
          45_000,
        );
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
};
const json = async <T>(url: string, init?: RequestInit): Promise<T> => {
  const response = await request(url, init);
  const body = await response.json();
  if (!response.ok) throw new Error(`${response.status}: ${JSON.stringify(body)}`);
  return body as T;
};
const createTable = (name: string, columns: Array<{ name: string; data_type: string }>) =>
  json<{ id: string; columns: Array<{ id: string; name: string; data_type: string }> }>(
    '/api/v1/dynamic-tables',
    {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ name, columns }),
    },
  );
const upload = (content: string, filename: string) => {
  const form = new FormData();
  form.set('file', new File([content], filename));
  return request('/api/v1/dynamic-tables/import', { method: 'POST', body: form });
};

try {
  runtime = await createApp(root, new TestAccess());
  const csv = await upload(
    `title,count\n${Array.from({ length: 61 }, (_, i) => `row-${i},${i}`).join('\n')}`,
    'source.csv',
  );
  assert.equal(csv.status, 201, await csv.clone().text());
  const importedCsv = (await csv.json()) as {
    id: string;
    name: string;
    columns: Array<{ name: string; data_type: string }>;
  };
  assert.equal(
    importedCsv.name,
    'source',
    'an omitted name should default to the uploaded filename',
  );
  assert.equal(importedCsv.columns.find((column) => column.name === 'count')?.data_type, 'float');
  const importedCsvRows = await json<{ total: number }>(
    `/api/v1/dynamic-tables/${importedCsv.id}/rows?page=1&size=100`,
  );
  assert.equal(importedCsvRows.total, 61);
  const beforeSingleColumnCsv = await json<{ tables: Array<{ id: string }> }>(
    '/api/v1/dynamic-tables?page=1&size=100',
  );
  const oneColumnCsv = await upload('name\nAda\n', 'one-column.csv');
  assert.equal(oneColumnCsv.status, 400);
  assert.equal(
    ((await oneColumnCsv.json()) as { error: { code: string } }).error.code,
    'csv.parse_error',
  );
  const afterSingleColumnCsv = await json<{ tables: Array<{ id: string }> }>(
    '/api/v1/dynamic-tables?page=1&size=100',
  );
  assert.equal(afterSingleColumnCsv.tables.length, beforeSingleColumnCsv.tables.length);

  const book = XLSX.utils.book_new();
  XLSX.utils.book_append_sheet(
    book,
    XLSX.utils.aoa_to_sheet([
      ['Name', 'Count'],
      ['Ada', 42],
    ]),
    'Creators',
  );
  const xlsxBytes = XLSX.write(book, { type: 'array', bookType: 'xlsx' });
  const xlsxForm = new FormData();
  xlsxForm.set('file', new File([xlsxBytes], 'creators.xlsx'));
  xlsxForm.set('name', 'XLSX Import');
  xlsxForm.set('sheet_name', 'Creators');
  const xlsxResponse = await request('/api/v1/dynamic-tables/import', {
    method: 'POST',
    body: xlsxForm,
  });
  assert.equal(xlsxResponse.status, 201, await xlsxResponse.clone().text());
  const xlsxTable = (await xlsxResponse.json()) as {
    id: string;
    columns: Array<{ name: string; data_type: string }>;
  };
  assert.equal(xlsxTable.columns.length, 2);
  assert.equal(
    xlsxTable.columns.find((column) => column.name === 'Count')?.data_type,
    'float',
    'numeric workbook cells should keep their inferred native number type',
  );
  const singleColumnWorkbook = XLSX.utils.book_new();
  XLSX.utils.book_append_sheet(
    singleColumnWorkbook,
    XLSX.utils.aoa_to_sheet([['Name'], ['Ada']]),
    'SingleColumn',
  );
  const singleColumnXlsxBytes = XLSX.write(singleColumnWorkbook, {
    type: 'array',
    bookType: 'xlsx',
  });
  const singleColumnXlsxForm = new FormData();
  singleColumnXlsxForm.set('file', new File([singleColumnXlsxBytes], 'single-column.xlsx'));
  const singleColumnXlsxResponse = await request('/api/v1/dynamic-tables/import', {
    method: 'POST',
    body: singleColumnXlsxForm,
  });
  assert.equal(singleColumnXlsxResponse.status, 201, await singleColumnXlsxResponse.clone().text());
  const singleColumnXlsxTable = (await singleColumnXlsxResponse.json()) as { id: string };
  const singleColumnXlsxRows = await json<{ total: number }>(
    `/api/v1/dynamic-tables/${singleColumnXlsxTable.id}/rows?page=1&size=100`,
  );
  assert.equal(singleColumnXlsxRows.total, 1);

  const capacityCsv = await upload(
    `value,count\n${Array.from({ length: 9_999 }, (_, index) => `fixture-${index},${index}`).join('\n')}`,
    'capacity.csv',
  );
  assert.equal(capacityCsv.status, 201, await capacityCsv.clone().text());
  const capacityTable = (await capacityCsv.json()) as {
    id: string;
    columns: Array<{ id: string; name: string }>;
  };
  const capacityValues = new Map(capacityTable.columns.map((column) => [column.name, column.id]));
  const capacityRowCount = async () =>
    (
      await json<{ total: number }>(
        `/api/v1/dynamic-tables/${capacityTable.id}/rows?page=1&size=100`,
      )
    ).total;
  assert.equal(await capacityRowCount(), 9_999);
  const overCapacityBatch = await request(`/api/v1/dynamic-tables/${capacityTable.id}/rows`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      rows: [10_000, 10_001].map((value) => ({
        values: { [capacityValues.get('value')!]: `overflow-${value}` },
      })),
    }),
  });
  assert.equal(overCapacityBatch.status, 400);
  assert.equal(await capacityRowCount(), 9_999, 'an overflowing batch must roll back every row');

  const writeOne = () =>
    request(`/api/v1/dynamic-tables/${capacityTable.id}/rows`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        rows: [{ values: { [capacityValues.get('value')!]: `concurrent-${Math.random()}` } }],
      }),
    });
  const concurrentRows = await Promise.all([writeOne(), writeOne()]);
  const concurrentRowResponses = await Promise.all(
    concurrentRows.map(async (response) => {
      const body = (await response
        .clone()
        .json()
        .catch(() => null)) as { error?: { code?: unknown; message?: unknown } } | null;
      const message = typeof body?.error?.message === 'string' ? body.error.message : undefined;
      return {
        status: response.status,
        code: typeof body?.error?.code === 'string' ? body.error.code : undefined,
        message: message
          ?.replace(/(?:postgres(?:ql)?:\/\/|password=|token=)\S+/gi, '[redacted]')
          .slice(0, 160),
      };
    }),
  );
  assert.equal(
    concurrentRows.filter((response) => response.status === 201).length,
    1,
    `concurrent row writes returned ${JSON.stringify(concurrentRowResponses)}`,
  );
  assert.equal(await capacityRowCount(), 10_000);
  const boundaryRow = (
    await json<{ rows: Array<{ id: string }> }>(
      `/api/v1/dynamic-tables/${capacityTable.id}/rows?page=1&size=1`,
    )
  ).rows[0]!;
  await request(`/api/v1/dynamic-tables/${capacityTable.id}/rows/${boundaryRow.id}`, {
    method: 'DELETE',
  });
  const afterDeleteWrite = await writeOne();
  assert.equal(afterDeleteWrite.status, 201);
  assert.equal(await capacityRowCount(), 10_000);

  const tooManyFields = Array.from({ length: 101 }, (_, index) => ({
    name: `Field ${index}`,
    data_type: 'string',
  }));
  assert.equal(
    (
      await request('/api/v1/dynamic-tables', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ name: 'Too many fields', columns: tooManyFields }),
      })
    ).status,
    400,
  );
  const fieldsAtNinetyNine = await createTable(
    'Fields at 99',
    Array.from({ length: 99 }, (_, index) => ({ name: `Field ${index}`, data_type: 'string' })),
  );
  const overflowingFields = await request(
    `/api/v1/dynamic-tables/${fieldsAtNinetyNine.id}/columns`,
    {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        columns: [
          { name: 'Field 100', data_type: 'string' },
          { name: 'Field 101', data_type: 'string' },
        ],
      }),
    },
  );
  assert.equal(overflowingFields.status, 400);
  assert.equal(
    (
      await json<{ columns: Array<{ id: string }> }>(
        `/api/v1/dynamic-tables/${fieldsAtNinetyNine.id}`,
      )
    ).columns.length,
    99,
    'an overflowing field batch must not leave a partial column',
  );
  const addField = (name: string) =>
    request(`/api/v1/dynamic-tables/${fieldsAtNinetyNine.id}/columns`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ columns: [{ name, data_type: 'string' }] }),
    });
  const concurrentFields = await Promise.all([addField('Concurrent A'), addField('Concurrent B')]);
  const concurrentFieldResponses = await Promise.all(
    concurrentFields.map(async (response) => {
      const body = (await response
        .clone()
        .json()
        .catch(() => null)) as { error?: { code?: unknown; message?: unknown } } | null;
      const message = typeof body?.error?.message === 'string' ? body.error.message : undefined;
      return {
        status: response.status,
        code: typeof body?.error?.code === 'string' ? body.error.code : undefined,
        message: message
          ?.replace(/(?:postgres(?:ql)?:\/\/|password=|token=)\S+/gi, '[redacted]')
          .slice(0, 160),
      };
    }),
  );
  assert.equal(
    concurrentFields.filter((response) => response.status === 201).length,
    1,
    `concurrent field writes returned ${JSON.stringify(concurrentFieldResponses)}`,
  );
  assert.equal(
    (
      await json<{ columns: Array<{ id: string }> }>(
        `/api/v1/dynamic-tables/${fieldsAtNinetyNine.id}`,
      )
    ).columns.length,
    100,
  );

  const httpServer = await new Promise<{ server: ReturnType<typeof serve>; port: number }>(
    (resolve) => {
      const server = serve(
        { fetch: (request) => runtime!.app.fetch(request), hostname: '127.0.0.1', port: 0 },
        (address) => resolve({ server, port: address.port }),
      );
    },
  );
  const mcpClient = new Client({ name: 'table-capacity-test', version: '1' });
  try {
    await mcpClient.connect(
      new StreamableHTTPClientTransport(new URL(`http://127.0.0.1:${httpServer.port}/mcp`)),
    );
    const mcpOverflow = await mcpClient.callTool({
      name: 'table_create_rows',
      arguments: {
        table_id: capacityTable.id,
        rows: [{ values: { [capacityValues.get('value')!]: 'from-mcp-overflow' } }],
      },
    });
    assert.equal(mcpOverflow.isError, true);
    assert.equal(await capacityRowCount(), 10_000);
  } finally {
    await mcpClient.close();
    await new Promise<void>((resolve, reject) =>
      httpServer.server.close((error) => (error ? reject(error) : resolve())),
    );
  }

  const table = await createTable('Export table', [
    { name: 'Label', data_type: 'string' },
    { name: 'Count', data_type: 'int' },
    { name: 'When', data_type: 'date' },
    { name: 'Enabled', data_type: 'bool' },
    { name: 'Website', data_type: 'website' },
    { name: 'Person', data_type: 'people' },
    { name: 'Score', data_type: 'score' },
  ]);
  const ids = new Map(table.columns.map((column) => [column.name, column.id]));
  const labelId = ids.get('Label')!;
  const personId = ids.get('Person')!;
  const scoreId = ids.get('Score')!;
  const exportRows = Array.from({ length: 101 }, (_, i) => ({
    values: {
      [labelId]: i === 0 ? '=1+1' : `row-${i}`,
      [ids.get('Count')!]: i,
      [ids.get('When')!]: '2026-09-23',
      [ids.get('Enabled')!]: i % 2 === 0,
      [ids.get('Website')!]: 'https://example.com/',
      [personId]: {
        name: 'Ada Lovelace',
        url: 'https://example.com/ada',
        image_url: 'https://example.com/avatar.png',
        identity: { platform: 'youtube', platform_id: 'ada-1' },
      },
      [scoreId]: { level: 'excellent', reason: 'Evidence was strong' },
    },
  }));
  for (let offset = 0; offset < exportRows.length; offset += 100) {
    await json(`/api/v1/dynamic-tables/${table.id}/rows`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ rows: exportRows.slice(offset, offset + 100) }),
    });
  }
  const renamed = await json<{ columns: Array<{ id: string; name: string }> }>(
    `/api/v1/dynamic-tables/${table.id}/columns/${labelId}`,
    {
      method: 'PATCH',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ name: 'Display name' }),
    },
  );
  assert.equal(renamed.columns.find((column) => column.name === 'Display name')?.id, labelId);
  const duplicate = await request(`/api/v1/dynamic-tables/${table.id}/columns/${labelId}`, {
    method: 'PATCH',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ name: 'Count' }),
  });
  assert.equal(duplicate.status, 409);

  const exportedResponse = await request(`/api/v1/dynamic-tables/${table.id}/export?locale=en`);
  assert.equal(exportedResponse.status, 200);
  assert.match(
    exportedResponse.headers.get('content-disposition') ?? '',
    /filename\*=UTF-8''Export%20table\.xlsx/,
  );
  const exported = XLSX.read(await exportedResponse.arrayBuffer(), {
    type: 'array',
    cellDates: true,
  });
  const sheet = exported.Sheets.Data!;
  assert.equal(XLSX.utils.decode_range(sheet['!ref']!).e.r, 101);
  assert.equal(sheet.A2?.v, '=1+1');
  assert.equal(sheet.A2?.f, undefined);
  assert.equal(sheet.B2?.t, 'n');
  assert.equal(sheet.D2?.t, 'b');
  assert.equal(sheet.E2?.l?.Target, 'https://example.com/');
  assert.equal(sheet.F2?.v, 'Ada Lovelace');
  assert.equal(sheet.F2?.l?.Target, 'https://example.com/ada');
  assert.match(sheet.F2?.c?.[0]?.t ?? '', /platform_id/);
  assert.equal(sheet.G2?.v, 'Excellent');
  assert.equal(sheet.G2?.c?.[0]?.t, 'Evidence was strong');

  const other = await createTable('Other table', [{ name: 'Value', data_type: 'string' }]);
  const empty = await createTable('Empty export', [{ name: 'Value', data_type: 'string' }]);
  const emptyExport = await request(`/api/v1/dynamic-tables/${empty.id}/export`);
  assert.equal(emptyExport.status, 200);
  const emptyWorkbook = XLSX.read(await emptyExport.arrayBuffer(), { type: 'array' });
  assert.equal(emptyWorkbook.Sheets.Data?.A1?.v, 'Value');
  const foreign = await json<{ rows: Array<{ id: string }> }>(
    `/api/v1/dynamic-tables/${other.id}/rows`,
    {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ rows: [{ values: { [other.columns[0]!.id]: 'keep' } }] }),
    },
  );
  const wrongDelete = await request(
    `/api/v1/dynamic-tables/${table.id}/rows/${foreign.rows[0]!.id}`,
    { method: 'DELETE' },
  );
  assert.equal(wrongDelete.status, 404);
  const own = await json<{ rows: Array<{ id: string }> }>(
    `/api/v1/dynamic-tables/${table.id}/rows?page=1&size=1`,
  );
  const deleted = await json<{ deleted_ids: string[] }>(
    `/api/v1/dynamic-tables/${table.id}/rows/${own.rows[0]!.id}`,
    { method: 'DELETE' },
  );
  assert.deepEqual(deleted.deleted_ids, [own.rows[0]!.id]);

  const before = await json<{ tables: Array<{ id: string }> }>(
    '/api/v1/dynamic-tables?page=1&size=100',
  );
  assert.equal((await upload('a,b\n"unterminated,1', 'bad-quotes.csv')).status, 400);
  assert.equal((await upload('name\nAda,Lovelace', 'wide-row.csv')).status, 400);
  assert.equal(
    (
      await request('/api/v1/dynamic-tables/import', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({}),
      })
    ).status,
    400,
  );
  assert.equal((await upload('bad workbook', 'bad.xlsx')).status, 400);
  const tooMany = await upload(
    `name,count\n${Array.from({ length: 10_001 }, (_, i) => `row-${i},${i}`).join('\n')}`,
    'too-many.csv',
  );
  assert.equal(tooMany.status, 400);
  assert.equal(
    ((await tooMany.json()) as { error: { code: string } }).error.code,
    'validation.row_limit',
  );
  const tooLargeForm = new FormData();
  tooLargeForm.set('file', new File(['x'.repeat(10 * 1024 * 1024 + 1)], 'too-large.csv'));
  assert.equal(
    (await request('/api/v1/dynamic-tables/import', { method: 'POST', body: tooLargeForm })).status,
    413,
  );
  const after = await json<{ tables: Array<{ id: string }> }>(
    '/api/v1/dynamic-tables?page=1&size=100',
  );
  assert.equal(after.tables.length, before.tables.length);

  await runtime.closeMcp();
  await runtime.data.close();
  runtime = await createApp(root, new TestAccess());
  const persisted = await json<{ total: number }>(
    `/api/v1/dynamic-tables/${table.id}/rows?page=1&size=100`,
  );
  assert.equal(persisted.total, 100);
  const persistedCsv = await json<{ total: number }>(
    `/api/v1/dynamic-tables/${importedCsv.id}/rows?page=1&size=100`,
  );
  const persistedXlsx = await json<{ total: number }>(
    `/api/v1/dynamic-tables/${xlsxTable.id}/rows?page=1&size=100`,
  );
  assert.equal(persistedCsv.total, 61);
  assert.equal(persistedXlsx.total, 1);
  console.log(
    'table files integration ok: CSV/XLSX import, complete export, cell fidelity, rename, delete, limits, and restart persistence',
  );
} finally {
  clearTimeout(totalTimeout);
  if (runtime) {
    await runtime.closeMcp();
    await runtime.data.close();
  }
  await rm(root, { recursive: true, force: true });
}
