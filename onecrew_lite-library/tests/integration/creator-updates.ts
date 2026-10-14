import assert from 'node:assert/strict';
import { once } from 'node:events';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { serve } from '@hono/node-server';
import { createApp } from '../../src/app.js';
import { notifyDiscoveryChanged } from '../../src/modules/tables/discovery-events.js';
import { TestAccess } from '../helpers/test-access.js';

const prefix = path.join(os.tmpdir(), 'onecrew-update-stream-');
const root = await mkdtemp(prefix);
const access = new TestAccess({ id: 'stream-alice', email: 'alice@example.test' });
const runtime = await createApp(root, access);
const server = serve({ fetch: runtime.app.fetch, hostname: '127.0.0.1', port: 0 });
const controller = new AbortController();
let reader: ReadableStreamDefaultReader<Uint8Array> | undefined;
try {
  if (!server.listening) await once(server, 'listening');
  const address = server.address();
  assert(address && typeof address !== 'string');
  const tenant = await runtime.tenants.resolve(access.user);
  const table = await runtime.data.createTable(tenant, 'Stream acceptance', []);
  const response = await fetch(
    `http://127.0.0.1:${address.port}/api/v1/dynamic-tables/${table.id}/creator-updates`,
    { signal: AbortSignal.any([controller.signal, AbortSignal.timeout(10000)]) },
  );
  assert.equal(response.status, 200);
  reader = response.body!.getReader();
  const first = await reader.read();
  assert.equal(new TextDecoder().decode(first.value), '{"changed":true}\n');
  const next = reader.read();
  notifyDiscoveryChanged('another-tenant', table.id);
  notifyDiscoveryChanged(tenant.tenantId, 'another-table');
  assert.equal(await Promise.race([next.then(() => 'unexpected'), delay(1000, 'idle')]), 'idle');
  notifyDiscoveryChanged(tenant.tenantId, table.id);
  const update = await Promise.race([
    next,
    delay(2000).then(() => {
      throw new Error('missing update');
    }),
  ]);
  assert.equal(new TextDecoder().decode(update.value), '{"changed":true}\n');
  const stopped = reader.read();
  assert.equal(await Promise.race([stopped.then(() => 'unexpected'), delay(1000, 'idle')]), 'idle');
  const aborted = assert.rejects(stopped, { name: 'AbortError' });
  controller.abort();
  await aborted;
  reader.releaseLock();
  reader = undefined;
  console.log(
    'Creator update real HTTP stream: initial snapshot, idle silence before/after publication, tenant/table isolation, publication notification and abort PASS',
  );
} finally {
  await reader?.cancel();
  controller.abort();
  await new Promise<void>((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()));
    if ('closeAllConnections' in server) server.closeAllConnections();
  });
  await runtime.closeMcp();
  runtime.browsers.closeAll();
  await runtime.data.close();
  assert.ok(path.resolve(root).startsWith(path.resolve(prefix)));
  await rm(root, { recursive: true, force: true });
}
