import { Hono } from 'hono';
import { createBrowserRoutes } from '../../src/modules/browser/http.js';
import { BrowserBridgeRegistry, BrowserBridgeService } from '../../src/modules/browser/service.js';
import type { TenantHttpEnv } from '../../src/modules/tenancy/service.js';
const id = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';
const bridge = new BrowserBridgeService(20);
const session = bridge.connect(id);
const httpBridge = new BrowserBridgeRegistry(() => new BrowserBridgeService(20));
const http = new Hono<TenantHttpEnv>();
http.use('*', async (c, next) => {
  c.set('tenant', {
    tenantId: 'browser-test',
    baseId: 'browser-base',
    user: { id: 'browser-user', email: 'browser@example.test' },
  });
  await next();
});
http.route('/api/v1/browser', createBrowserRoutes(httpBridge));
const preflight = await http.request('http://127.0.0.1:18092/api/v1/browser/commands', {
  method: 'OPTIONS',
  headers: {
    Origin: `chrome-extension://${id}`,
    'Access-Control-Request-Headers': 'Authorization, X-OneCrew-Extension-Id',
  },
});
if (preflight.status !== 204) throw new Error(`extension preflight failed: ${preflight.status}`);
const connectResponse = await http.request('http://127.0.0.1:18092/api/v1/browser/connect', {
  method: 'POST',
  headers: {
    'Content-Type': 'application/json',
    Host: '127.0.0.1:18092',
    'X-OneCrew-Extension-Id': id,
  },
  body: JSON.stringify({ extension_id: id }),
});
if (connectResponse.status !== 200 || !((await connectResponse.json()) as { token?: string }).token)
  throw new Error('HTTP connect failed');
if (!session.token || !bridge.status().paired) throw new Error('connect failed');
const work = bridge.dispatch('youtube.discover_kols', {
  query: 'onecrew',
  limit: 2,
});
if (bridge.connect(id).token !== session.token) throw new Error('reconnect rotated active token');
let rejectedOther = false;
try {
  bridge.connect('bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb');
} catch {
  rejectedOther = true;
}
if (!rejectedOther) throw new Error('another extension replaced active session');
const command = await bridge.nextCommand(bridge.authenticate(session.token, id));
if (!command || command.operation !== 'youtube.discover_kols')
  throw new Error('command poll failed');
if (!bridge.heartbeat(session.token, id).ok) throw new Error('heartbeat authentication failed');
bridge.submitResult({
  id: command.id,
  result: { channels: [], has_more: false, stop_reason: 'exhausted', warnings: [] },
});
const result = await work;
if (!result || (result as { channels: unknown[] }).channels.length !== 0)
  throw new Error('result completion failed');
if (
  bridge.submitResult({
    id: command.id,
    result: { channels: [], has_more: false, stop_reason: 'exhausted', warnings: [] },
  }).accepted
)
  throw new Error('replayed result was accepted');
const timed = bridge.dispatch('youtube.discover_kols', { query: 'timeout', limit: 1 }).then(
  () => {
    throw new Error('timeout did not reject');
  },
  (error: Error) => {
    if (!error.message.includes('timed out')) throw error;
  },
);
await bridge.nextCommand(bridge.authenticate(session.token, id));
await timed;
console.log('browser bridge integration ok: connect, authenticated poll, result replay, timeout');
