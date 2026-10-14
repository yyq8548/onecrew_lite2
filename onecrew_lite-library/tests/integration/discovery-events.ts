import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { createApp } from '../../src/app.js';
import { notifyDiscoveryChanged } from '../../src/modules/tables/discovery-events.js';
import { TestAccess } from '../helpers/test-access.js';

const root = await mkdtemp(path.join(os.tmpdir(), 'onecrew-table-events-'));
const access = new TestAccess({ id: 'event-alice', email: 'alice@example.test' });
const runtime = await createApp(root, access);
try {
  const tenant = await runtime.tenants.resolve(access.user);
  const table = await runtime.data.createTable(tenant, 'Results', [
    { name: 'Notes', data_type: 'string' },
  ]);
  const request = () =>
    runtime.app.request(`http://127.0.0.1/api/v1/dynamic-tables/${table.id}/creator-updates`, {
      headers: { Host: '127.0.0.1', 'X-OneCrew-Account-Id': access.user.id },
    });
  const response = await request();
  assert.equal(response.status, 200);
  const reader = response.body!.getReader();
  const decode = new TextDecoder();
  assert.equal(decode.decode((await reader.read()).value), '{"changed":true}\n');
  let delivered = false;
  const next = reader.read().then((value) => {
    delivered = true;
    return value;
  });
  notifyDiscoveryChanged('another-tenant', table.id);
  notifyDiscoveryChanged(tenant.tenantId, 'another-table');
  await new Promise((resolve) => setTimeout(resolve, 30));
  assert.equal(delivered, false, 'idle and unrelated changes do not refresh a table');
  notifyDiscoveryChanged(tenant.tenantId, table.id);
  assert.equal(decode.decode((await next).value), '{"changed":true}\n');
  await reader.cancel();
  access.user = { id: 'event-bob', email: 'bob@example.test' };
  assert.equal((await request()).status, 404, 'events preserve table ownership');
  console.log(
    'Discovery events: idle silence, publication invalidation, tenant/table isolation and cancellation PASS',
  );
} finally {
  await runtime.closeMcp();
  runtime.browsers.closeAll();
  await runtime.data.close();
  await rm(root, { recursive: true, force: true });
}
