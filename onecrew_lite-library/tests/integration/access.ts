import assert from 'node:assert/strict';
import { AccessClient, type Candidate, type Credential } from '../../src/modules/access/client.js';
import { AccessService, type CredentialStore } from '../../src/modules/access/service.js';
import { createAccessCallback, createAccessRoutes } from '../../src/modules/access/http.js';
import { createSystemRoutes, collectSystemInfo, getDeviceLabel } from '../../src/system.js';
import { APP_VERSION } from '../../src/version.js';

let stored: Credential | undefined;
const store: CredentialStore = {
  async load() {
    return stored;
  },
  async save(value) {
    stored = value;
  },
  async clear() {
    stored = undefined;
  },
};
let enrollments = 0;
let reports = 0;
class TestClient extends AccessClient {
  override async enroll(candidate: Candidate): Promise<Credential> {
    enrollments++;
    return {
      bearer: candidate.bearer,
      installation: {
        ...candidate,
        access_user_id: 'user-test',
        access_device_id: `dev_${'a'.repeat(24)}`,
        access_pairing_id: `pair_${'b'.repeat(24)}`,
        source_instance_id: candidate.installation_id,
        status: 'active',
        revision: 1,
        created_at: 1,
        updated_at: 1,
        last_seen_at: null,
        revoked_at: null,
      },
    };
  }
  override async validate() {}
  override async reportSystemInfo() {
    reports++;
  }
  override async profile() {
    return { id: 'user-test', email: 'test@example.com' };
  }
}
const service = new AccessService(store, new TestClient(), 'darwin');
assert.equal((await service.status()).state, 'signed_out');
const login = new URL((await service.begin(8091)).authorize_url);
assert.equal(login.searchParams.get('app_version'), APP_VERSION);
assert.equal(login.searchParams.get('code_challenge_method'), 'S256');
assert.equal(login.searchParams.get('callback_port'), '8091');
assert.equal(login.searchParams.get('label'), await getDeviceLabel());
await assert.rejects(service.complete(`reacher_pairing_v1.${'c'.repeat(43)}`, 'bad-state'));
assert.equal(enrollments, 0);
const callback = createAccessCallback(service);
const callbackUrl = new URL('http://127.0.0.1:8091/creator-sync/pairing-callback');
callbackUrl.search = new URLSearchParams({
  code: `reacher_pairing_v1.${'c'.repeat(43)}`,
  state: login.searchParams.get('state')!,
}).toString();
const completed = await callback.request(callbackUrl.toString());
assert.equal(completed.status, 303);
assert.equal(completed.headers.get('location'), '/auth/complete');
assert.equal(completed.headers.get('cache-control'), 'no-store');
assert.equal(completed.headers.get('referrer-policy'), 'no-referrer');
const replayed = await callback.request(callbackUrl.toString());
assert.equal(replayed.status, 400);
assert.equal(replayed.headers.get('location'), null);
assert.equal((await service.status()).user?.email, 'test@example.com');
await service.status();
assert.equal(reports, 1);
await assert.rejects(
  service.complete(`reacher_pairing_v1.${'c'.repeat(43)}`, login.searchParams.get('state')!),
);
assert.equal(enrollments, 1);
assert.equal((await service.logout()).state, 'signed_out');
assert.equal(stored, undefined);
const canceled = new URL((await service.begin(8091)).authorize_url);
await service.logout();
await assert.rejects(
  service.complete(`reacher_pairing_v1.${'c'.repeat(43)}`, canceled.searchParams.get('state')!),
);
const routes = createAccessRoutes(service);
assert.equal((await routes.request('http://127.0.0.1:8091/login', { method: 'POST' })).status, 403);
assert.equal(
  (
    await routes.request('http://127.0.0.1:8091/status', {
      headers: { Origin: 'https://example.com' },
    })
  ).status,
  403,
);
const status = await routes.request('http://127.0.0.1:8091/status');
assert.equal(status.status, 200);
assert.equal(status.headers.get('cache-control'), 'no-store');
const system = createSystemRoutes();
assert.equal(
  (
    await system.request('http://127.0.0.1:8091/info', {
      headers: { Origin: 'https://example.com' },
    })
  ).status,
  403,
);
const info = await collectSystemInfo();
assert.equal(info.runtime.arch, process.arch);
assert.equal(info.runtime.node, process.versions.node);
assert.deepEqual(Object.keys(info.os).sort(), [
  'build',
  'kernel',
  'name',
  'native_arch',
  'platform',
  'version',
]);
assert.ok(info.os.version);
console.log('Access state, replay, logout, origin guards and local diagnostics passed.');

// Exercise the real remote client contract against an isolated HTTP server.
const { serve } = await import('@hono/node-server');
const { Hono } = await import('hono');
const { randomUUID } = await import('node:crypto');
const candidate: Candidate = {
  installation_id: randomUUID(),
  credential_id: randomUUID(),
  bearer: 'test-bearer'.padEnd(80, 'x'),
  client_platform: 'macos',
  app_version: '0.1.0',
};
const { bearer: _bearer, ...installationFields } = candidate;
void _bearer;
const installation = {
  ...installationFields,
  access_user_id: 'contract-user',
  access_device_id: `dev_${'a'.repeat(24)}`,
  access_pairing_id: `pair_${'b'.repeat(24)}`,
  source_instance_id: randomUUID(),
  status: 'active',
  revision: 1,
  created_at: 1,
  updated_at: 1,
  last_seen_at: null,
  revoked_at: null,
};
const remote = new Hono();
remote.post('/installations', async (c) => {
  const payload = await c.req.json();
  assert.equal(payload.pairing_code, `reacher_pairing_v1.${'a'.repeat(43)}`);
  assert.equal(payload.code_verifier, 'v'.repeat(43));
  assert.equal(c.req.header('Idempotency-Key'), 'request-id-1234567');
  return c.json({ installation, replayed: false }, 201);
});
remote.use('*', async (c, next) => {
  assert.equal(c.req.header('Authorization'), `Bearer ${candidate.bearer}`);
  await next();
});
remote.get('/installations/:id', (c) => c.json({ installation }));
remote.get('/creator-sync/credential', (c) =>
  c.json({
    protocol: 'reacher.creator_sync',
    protocol_version: 1,
    installation_id: installation.installation_id,
    credential_id: installation.credential_id,
    access_user_id: installation.access_user_id,
    access_device_id: installation.access_device_id,
    access_pairing_id: installation.access_pairing_id,
    source_instance_id: installation.source_instance_id,
    capabilities: ['creator_snapshots:write_chunked_quarantine'],
    accepted_bundle: {
      format: 'test',
      format_version: 3,
      profile: 'portable_public',
      max_snapshot_bytes: 100,
      max_chunk_bytes: 100,
      max_chunks: 1,
      max_namespace_staged_bytes: 100,
      max_access_user_staged_bytes: 100,
    },
  }),
);
let differentUser = false;
remote.get('/me', (c) =>
  c.json({
    id: differentUser ? 'different-user' : installation.access_user_id,
    email: 'test@example.com',
  }),
);
let received = false;
remote.put('/device-info', async (c) => {
  assert.deepEqual(await c.req.json(), info);
  received = true;
  return c.json({ ok: true });
});
const server = serve({ fetch: remote.fetch, hostname: '127.0.0.1', port: 0 });
await new Promise<void>((resolve) => server.once('listening', resolve));
try {
  const address = server.address();
  assert.ok(address && typeof address === 'object');
  const client = new AccessClient(`http://127.0.0.1:${address.port}`);
  const credential = await client.enroll(
    candidate,
    `reacher_pairing_v1.${'a'.repeat(43)}`,
    'v'.repeat(43),
    'request-id-1234567',
  );
  assert.equal((await client.profile(credential)).id, installation.access_user_id);
  await client.reportSystemInfo(credential, info);
  assert.ok(received);
  differentUser = true;
  await assert.rejects(client.profile(credential), { code: 'access.invalid_response' });
  console.log('Remote enrollment, profile identity binding and diagnostics contract passed.');
} finally {
  if ('closeAllConnections' in server) server.closeAllConnections();
  await new Promise<void>((resolve) => server.close(() => resolve()));
}

// Verify the actual app boundary, not just the account UI.
const { mkdtemp, rm } = await import('node:fs/promises');
const { tmpdir } = await import('node:os');
const path = await import('node:path');
const { createApp } = await import('../../src/app.js');
const { TestAccess } = await import('../helpers/test-access.js');
const temp = await mkdtemp(path.join(tmpdir(), 'onecrew-auth-gate-'));
const testIdentity = new TestAccess();
testIdentity.state = 'signed_out';
const runtime = await createApp(temp, testIdentity);
const origin = 'http://127.0.0.1:18095';
try {
  for (const [pathname, method] of [
    ['/api/v1/dynamic-tables', 'POST'],
    ['/api/v1/dynamic-tables', 'GET'],
    ['/api/v1/searches', 'POST'],
    ['/api/v1/browser/connect', 'POST'],
  ]) {
    const response = await runtime.app.request(`${origin}${pathname}`, { method });
    assert.equal(response.status, 401, pathname);
    assert.equal((await response.json()).error.code, 'access.login_required');
  }
  for (const pathname of ['/library', '/library/test-table?q=coffee%20maker', '/connections']) {
    const response = await runtime.app.request(`${origin}${pathname}`);
    assert.equal(response.status, 302);
    assert.equal(
      response.headers.get('location'),
      `/login?returnTo=${encodeURIComponent(pathname)}`,
    );
  }
  assert.equal((await runtime.app.request(`${origin}/api/v1/access/status`)).status, 200);
  assert.equal((await runtime.app.request(`${origin}/healthz`)).status, 200);
  for (const state of ['signed_out', 'unavailable', 'signed_in'] as const) {
    testIdentity.state = state;
    const completion = await runtime.app.request(`${origin}/auth/complete`);
    assert.equal(completion.status, 200);
    assert.match(completion.headers.get('content-type') ?? '', /text\/html/);
  }
  testIdentity.state = 'signed_in';
  assert.equal((await runtime.app.request(`${origin}/api/v1/dynamic-tables`)).status, 200);
  await testIdentity.logout();
  assert.equal((await runtime.app.request(`${origin}/api/v1/dynamic-tables`)).status, 401);
  testIdentity.state = 'unavailable';
  assert.equal((await runtime.app.request(`${origin}/api/v1/dynamic-tables`)).status, 503);
  const { Client, StreamableHTTPClientTransport } = await import('@modelcontextprotocol/client');
  const listener = serve({ fetch: runtime.app.fetch, hostname: '127.0.0.1', port: 0 });
  await new Promise<void>((resolve) => listener.once('listening', resolve));
  const address = listener.address();
  assert.ok(address && typeof address === 'object');
  try {
    for (const mode of ['legacy', { pin: '2026-07-28' }] as const) {
      testIdentity.state = 'signed_out';
      const client = new Client(
        { name: 'auth-gate-test', version: '1' },
        { versionNegotiation: { mode } },
      );
      try {
        await client.connect(
          new StreamableHTTPClientTransport(new URL(`http://127.0.0.1:${address.port}/mcp`)),
        );
        assert.ok((await client.listTools()).tools.some((tool) => tool.name === 'account'));
        const denied = await client.callTool({ name: 'table_list', arguments: {} });
        assert.equal(denied.isError, true);
        const login = await client.callTool({ name: 'account', arguments: { action: 'login' } });
        assert.ok(JSON.stringify(login).includes(`http://127.0.0.1:${address.port}/login`));
        testIdentity.state = 'signed_in';
        const allowed = await client.callTool({ name: 'table_list', arguments: {} });
        assert.notEqual(allowed.isError, true);
        await client.callTool({ name: 'account', arguments: { action: 'logout' } });
        assert.equal(testIdentity.state, 'signed_out');
        assert.equal((await client.callTool({ name: 'table_list', arguments: {} })).isError, true);
      } finally {
        await client.close();
      }
    }
  } finally {
    if ('closeAllConnections' in listener) listener.closeAllConnections();
    await new Promise<void>((resolve) => listener.close(() => resolve()));
  }
  console.log(
    'HTTP login gate and MCP account discovery/login/logout in both protocol modes passed.',
  );
} finally {
  runtime.browsers.closeAll();
  await runtime.closeMcp();
  await runtime.data.close();
  await rm(temp, { recursive: true, force: true });
}
