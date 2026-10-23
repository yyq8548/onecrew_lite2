import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { mkdir, mkdtemp, rm } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createEmbeddedTables } from '../../src/embedded.js';

const scratch = fileURLToPath(new URL('../../../.scratch/embedded-tables/', import.meta.url));
await mkdir(scratch, { recursive: true });
const root = await mkdtemp(path.join(scratch, 'database-'));
const firstUser = { id: randomUUID(), email: 'first@example.test' };
const secondUser = { id: randomUUID(), email: 'second@example.test' };
let tables: Awaited<ReturnType<typeof createEmbeddedTables>> | undefined;
try {
  tables = await createEmbeddedTables(root, 'http://127.0.0.1:34567');
  const first = await tables.tenants.resolve(firstUser);
  const second = await tables.tenants.resolve(secondUser);
  const table = await tables.service.createTable(first, {
    name: '兼容验证',
    columns: [{ name: '名称', data_type: 'string' }],
  });
  assert.equal((await tables.service.listTables(first)).tables[0]?.id, table.id);
  assert.equal((await tables.service.listTables(second)).tables.length, 0);
  await assert.rejects(tables.service.getTable(second, table.id), { status: 404 });
  await tables.close();
  tables = await createEmbeddedTables(root, 'http://127.0.0.1:34567');
  const restored = await tables.tenants.resolve(firstUser);
  assert.deepEqual(restored, first);
  assert.equal((await tables.service.getTable(restored, table.id)).name, '兼容验证');
  console.log('embedded tables: real PGlite isolation and restart passed');
} finally {
  if (tables) await tables.close();
  await rm(root, { recursive: true });
}
