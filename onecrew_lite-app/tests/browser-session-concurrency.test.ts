import assert from 'node:assert/strict';
import { randomBytes, randomUUID } from 'node:crypto';
import test from 'node:test';
import { z } from 'zod';
import { withBrowserSessionLock } from '../../onecrew_lite-contracts/src/browser-session-lock.ts';
import { OperationRegistry } from '../src/application/operation-registry.ts';
import { AccountContextService } from '../src/identity/account-context.ts';
import { BrowserSessions } from '../src/service/browser-sessions.ts';
import { listenLocalServer } from '../src/service/http.ts';

test('concurrent cold workbench and Library handshakes preserve one cookie and valid CSRF', async () => {
  const accounts = new AccountContextService();
  const context = accounts.activate({ user: { id: 'account', email: 'user@example.com' }, tenantId: 'tenant', baseId: 'base', deviceId: 'device', source: 'access' });
  const sessions = new BrowserSessions(accounts);
  const registry = new OperationRegistry(accounts);
  registry.register('local.identity.read', z.object({}).strict(), async (_, current) => ({ account: current.principal.user.id }));
  const server = await listenLocalServer({ port: 0, instanceId: randomUUID(), controlToken: randomBytes(32).toString('hex'), operations: registry, browserSessions: sessions });
  let cookie = '';
  async function connect(ticket: string) {
    return withBrowserSessionLock(async () => {
      const exchange = await fetch(`${server.origin}/api/local/session`, { method: 'POST', headers: { origin: server.origin, 'content-type': 'application/json', ...(cookie ? { cookie } : {}) }, body: JSON.stringify({ ticket }) });
      assert.equal(exchange.status, 200);
      cookie = exchange.headers.get('set-cookie')!.split(';')[0]!;
      const response = await fetch(`${server.origin}/api/session`, { headers: { cookie } });
      assert.equal(response.status, 200);
      return z.object({ ok: z.literal(true), result: z.object({ csrf: z.string(), accountId: z.string(), epoch: z.string() }) }).parse(await response.json()).result;
    });
  }
  try {
    const pages = await Promise.all([connect(sessions.issueTicket(context)), connect(sessions.issueTicket(context))]);
    for (const page of pages) {
      const response = await fetch(`${server.origin}/api/operations/local.identity.read`, { method: 'POST', headers: { cookie, origin: server.origin, 'content-type': 'application/json', 'x-onecrew-csrf': page.csrf, 'x-onecrew-account-id': page.accountId, 'x-onecrew-auth-epoch': page.epoch }, body: '{}' });
      assert.equal(response.status, 200, '两页必须都能通过最终共享 Cookie 的 CSRF 验证');
    }
    assert.equal(pages[0]!.csrf, pages[1]!.csrf);
  } finally { await server.close(); }
});
