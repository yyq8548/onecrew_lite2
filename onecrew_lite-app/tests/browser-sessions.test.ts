import assert from "node:assert/strict";
import { randomBytes, randomUUID } from "node:crypto";
import test from "node:test";
import { z } from "zod";
import { OperationRegistry } from "../src/application/operation-registry.ts";
import { AccountContextService } from "../src/identity/account-context.ts";
import { BrowserSessions } from "../src/service/browser-sessions.ts";
import { listenLocalServer } from "../src/service/http.ts";
import { WebAssets } from "../src/service/web-assets.ts";
import { fileURLToPath } from "node:url";

test("real HTTP browser session consumes tickets and enforces CSRF, origin and account epoch", async () => {
  const accounts = new AccountContextService();
  const principal = { user: { id: "account", email: "user@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" as const };
  const current = accounts.activate(principal);
  const sessions = new BrowserSessions(accounts);
  const registry = new OperationRegistry(accounts);
  registry.register("local.identity.read", z.object({}).strict(), async (_, context) => ({ account: context.principal.user.id }));
  const controlToken = randomBytes(32).toString("hex");
  const server = await listenLocalServer({ port: 0, instanceId: randomUUID(), controlToken, operations: registry, browserSessions: sessions, webAssets: await WebAssets.load(fileURLToPath(new URL("../../onecrew_lite-rust/apps/web/dist", import.meta.url))), issueBrowserTicket: async () => sessions.issueTicket(accounts.current()) });
  const ticket = sessions.issueTicket(current);
  const exchange = (value: string, origin = server.origin) => fetch(`${server.origin}/api/local/session`, { method: "POST", headers: { origin, "content-type": "application/json" }, body: JSON.stringify({ ticket: value }) });
  try {
    assert.equal((await fetch(`${server.origin}/_local/workspace`, { method: "POST" })).status, 401);
    const workspace = await fetch(`${server.origin}/_local/workspace`, { method: "POST", headers: { authorization: `Bearer ${controlToken}` } });
    assert.equal(workspace.status, 200);
    const workspaceUrl = new URL((await workspace.json()).url);
    assert.equal(workspaceUrl.origin, server.origin);
    const workspaceTicket = new URLSearchParams(workspaceUrl.hash.slice(1)).get("onecrew_ticket")!;
    assert.equal((await exchange(workspaceTicket)).status, 200);
    assert.equal((await exchange(workspaceTicket)).status, 401);
    assert.equal((await exchange(ticket, "https://hostile.example")).status, 403);
    const response = await exchange(ticket);
    assert.equal(response.status, 200);
    const setCookie = response.headers.get("set-cookie")!;
    assert.match(setCookie, /HttpOnly/u);
    assert.match(setCookie, /SameSite=Strict/u);
    const cookie = setCookie.split(";")[0]!;
    const session = z.object({ csrf: z.string(), accountId: z.string(), epoch: z.string() }).parse(await response.json());
    const handshake = await fetch(`${server.origin}/api/session`, { headers: { cookie } });
    assert.equal(handshake.status, 200);
    assert.deepEqual(await handshake.json(), { ok: true, result: { ...session, version: "0.7.1", transport: "local_http", protocol_version: 2 } });
    assert.equal((await exchange(ticket)).status, 401);
    const headers = { cookie, origin: server.origin, "content-type": "application/json", "x-onecrew-account-id": session.accountId, "x-onecrew-auth-epoch": session.epoch };
    const invoke = (extra: Record<string, string>) => fetch(`${server.origin}/api/operations/local.identity.read`, { method: "POST", headers: { ...headers, ...extra }, body: "{}" });
    assert.equal((await invoke({})).status, 401);
    assert.equal((await invoke({ "x-onecrew-csrf": session.csrf, origin: "https://hostile.example" })).status, 403);
    assert.equal((await invoke({ "x-onecrew-csrf": session.csrf })).status, 200);
    const malformed = await fetch(`${server.origin}/api/operations/local.identity.read`, { method: "POST", headers: { ...headers, "x-onecrew-csrf": session.csrf }, body: new Uint8Array([123, 34, 120, 34, 58, 34, 128, 34, 125]) });
    assert.equal(malformed.status, 400);
    const command = await fetch(`${server.origin}/api/commands/local.identity.read`, { method: "POST", headers: { ...headers, "x-onecrew-csrf": session.csrf }, body: "{}" });
    assert.deepEqual(await command.json(), { ok: true, result: { account: "account" } });
    assert.equal((await fetch(`${server.origin}/_local/status`, { headers: { cookie } })).status, 401);
    assert.equal((await invoke({ "x-onecrew-csrf": session.csrf, "x-onecrew-account-id": "other-account" })).status, 409);
    accounts.activate(principal);
    assert.equal((await invoke({ "x-onecrew-csrf": session.csrf })).status, 401);
    sessions.clear();
  } finally { await server.close(); }
});

test("two loopback profiles keep independent cookies in the same browser cookie header", async () => {
  const accounts = new AccountContextService();
  const current = accounts.activate({ user: { id: "account", email: "user@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" });
  const firstSessions = new BrowserSessions(accounts);
  const secondSessions = new BrowserSessions(accounts);
  const options = { port: 0, instanceId: randomUUID(), controlToken: randomBytes(32).toString("hex"), operations: new OperationRegistry(accounts) };
  const first = await listenLocalServer({ ...options, browserSessions: firstSessions });
  const second = await listenLocalServer({ ...options, instanceId: randomUUID(), browserSessions: secondSessions });
  try {
    async function cookieFor(server: typeof first, sessions: BrowserSessions) {
      const response = await fetch(`${server.origin}/api/local/session`, { method: "POST", headers: { origin: server.origin, "content-type": "application/json" }, body: JSON.stringify({ ticket: sessions.issueTicket(current) }) });
      assert.equal(response.status, 200);
      return response.headers.get("set-cookie")!.split(";")[0]!;
    }
    const a = await cookieFor(first, firstSessions), b = await cookieFor(second, secondSessions);
    assert.notEqual(a.split("=")[0], b.split("=")[0]);
    for (const server of [first, second]) {
      const response = await fetch(`${server.origin}/api/session`, { headers: { cookie: `${a}; ${b}` } });
      assert.equal(response.status, 200);
      assert.equal((await response.json()).result.accountId, "account");
    }
  } finally { await Promise.all([first.close(), second.close()]); }
});
