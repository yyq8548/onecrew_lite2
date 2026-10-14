// Manual browser acceptance of the real built Library, with synthetic data only.
// Explicit opt-in, loopback, temporary storage and a five-minute lifetime.
import assert from 'node:assert/strict';
import { once } from 'node:events';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { serve } from '@hono/node-server';
import { Hono } from 'hono';
import { createApp } from '../../src/app.js';
import { notifyDiscoveryChanged } from '../../src/modules/tables/discovery-events.js';
import { TestAccess } from '../helpers/test-access.js';

assert.equal(process.env.ONECREW_TEST_UI_ACCEPTANCE, 'true', 'Explicit browser acceptance opt-in');
const root = await mkdtemp(path.join(os.tmpdir(), 'onecrew-ui-refresh-'));
const access = new TestAccess({ id: 'ui-refresh-fixture', email: 'ui@example.test' });
const runtime = await createApp(root, access);
let server: ReturnType<typeof serve> | undefined;
let timeout: ReturnType<typeof setTimeout> | undefined;
try {
  const tenant = await runtime.tenants.resolve(access.user);
  const table = await runtime.data.ensureDiscoveryCampaignTable(tenant, {
    campaign_id: 'ui-refresh-fixture',
    name: '隔离刷新验收_1005_1',
    columns: [{ name: 'Match evaluation', data_type: 'score' }],
  });
  const counts: Record<string, number> = {};
  const source = {
    search_hit_id: 'fixture-hit',
    candidate_id: 'fixture-candidate',
    native_author_id: 'fixture-native-author',
    evidence_version: 'a'.repeat(64),
    contacts: [],
  };
  await runtime.data.ingestDiscoverySources(tenant, {
    table_id: table.id,
    campaign_id: 'ui-refresh-fixture',
    criteria_revision: 1,
    source_revision: 1,
    rows: [{ ...source, verdict: 'pending', reasons: ['language_unverified'], values: {} }],
  });
  await runtime.data.publishDiscoveryIdentity(tenant, {
    table_id: table.id,
    campaign_id: 'ui-refresh-fixture',
    projection_format: 1,
    criteria_revision: 1,
    source_revision: 1,
    source_hash: 'b'.repeat(64),
    identity_revision: 1,
    identity_hash: 'c'.repeat(64),
    source_rows: [
      {
        search_hit_id: source.search_hit_id,
        candidate_id: source.candidate_id,
        native_author_id: source.native_author_id,
        evidence_version: source.evidence_version,
      },
    ],
    groups: [
      {
        group_id: 'd'.repeat(64),
        has_same_author_review: false,
        decision_ids: [],
        members: [
          {
            candidate_id: source.candidate_id,
            native_author_id: source.native_author_id,
            platform: 'fixture',
            display_name: '隔离测试候选',
            profile_url: 'https://example.test/creator',
            source_hit_ids: [source.search_hit_id],
          },
        ],
      },
    ],
  });
  let stop!: () => void;
  const finished = new Promise<void>((resolve) => {
    stop = resolve;
  });
  const app = new Hono()
    .get('/_test/counts', (c) => c.json(counts))
    .post('/_test/disconnect', (c) => {
      // Interrupt real browser streams while keeping the isolated server alive.
      // This exercises client reconnect after transport loss, not a mocked event.
      setTimeout(() => {
        assert(
          server && 'closeAllConnections' in server,
          'This HTTP receiver must support disconnects',
        );
        server.closeAllConnections();
      }, 25);
      return c.json({ disconnected: true });
    })
    .post('/_test/notify', (c) => {
      notifyDiscoveryChanged(tenant.tenantId, table.id);
      return c.json({ notified: true });
    })
    .post('/_test/stop', (c) => {
      setImmediate(stop);
      return c.json({ stopped: true });
    })
    .all('*', (c) => {
      const key = `${c.req.method} ${c.req.path}`;
      counts[key] = (counts[key] ?? 0) + 1;
      return runtime.app.fetch(c.req.raw);
    });
  server = serve({ fetch: app.fetch, hostname: '127.0.0.1', port: 0 });
  timeout = setTimeout(stop, 300000);
  if (!server.listening) await once(server, 'listening');
  const address = server.address();
  assert(address && typeof address !== 'string');
  console.log(JSON.stringify({ url: `http://127.0.0.1:${address.port}/library/${table.id}` }));
  await finished;
} finally {
  if (timeout) clearTimeout(timeout);
  const activeServer = server;
  if (activeServer) {
    await new Promise<void>((resolve, reject) => {
      activeServer.close((error) => (error ? reject(error) : resolve()));
      if ('closeAllConnections' in activeServer) activeServer.closeAllConnections();
    });
  }
  await runtime.closeMcp();
  runtime.browsers.closeAll();
  await runtime.data.close();
  assert.equal(path.dirname(root), os.tmpdir());
  assert(path.basename(root).startsWith('onecrew-ui-refresh-'));
  await rm(root, { recursive: true, force: true });
}
