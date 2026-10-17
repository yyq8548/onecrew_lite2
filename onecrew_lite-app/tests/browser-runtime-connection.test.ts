import assert from "node:assert/strict";
import { randomBytes, randomUUID } from "node:crypto";
import test from "node:test";
import { z } from "zod";
import { OperationRegistry } from "../src/application/operation-registry.ts";
import { AccountContextService } from "../src/identity/account-context.ts";
import { BrowserSessions } from "../src/service/browser-sessions.ts";
import { listenLocalServer } from "../src/service/http.ts";
import * as browserRuntime from "../../onecrew_lite-rust/apps/web/src/lib/local-runtime.ts";

test("browser transport keeps a valid session after a forbidden request and recovers after a real server interruption without replay", async () => {
  const accounts = new AccountContextService();
  const principal = { user: { id: "connection-test", email: "connection@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" as const };
  const context = accounts.activate(principal);
  const sessions = new BrowserSessions(accounts);
  const operations = new OperationRegistry(accounts);
  let reads = 0;
  operations.register("connection_read", z.strictObject({}), async () => ({ reads: ++reads }));
  const options = { port: 0, instanceId: randomUUID(), controlToken: randomBytes(32).toString("hex"), operations, browserSessions: sessions };
  let server = await listenLocalServer(options);
  const origin = server.origin;
  const nativeFetch = globalThis.fetch;
  const exchanged = await nativeFetch(`${origin}/api/local/session`, { method: "POST", headers: { origin, "content-type": "application/json" }, body: JSON.stringify({ ticket: sessions.issueTicket(context) }) });
  let cookie = exchanged.headers.get("set-cookie")!.split(";")[0]!;
  let handshakes = 0;
  // 仅补齐浏览器自动提供的相对地址、Origin 和 Cookie；全部响应来自真实 HTTP 服务。
  globalThis.fetch = (input, init) => {
    const url = new URL(String(input), origin);
    if (url.pathname === "/api/session") handshakes++;
    return nativeFetch(url, { ...init, headers: { origin, cookie, ...init?.headers } });
  };
  try {
    await browserRuntime.connectLocalRuntime();
    await assert.rejects(browserRuntime.runtimeRequest("/api/commands/connection_read", "{}", { "Content-Type": "application/json", origin: "https://hostile.example" }));
    assert.equal(browserRuntime.isRuntimeAvailable(), true, "单次请求的 403 不能清除有效会话");
    assert.deepEqual(await browserRuntime.invoke("connection_read"), { reads: 1 });
    await server.close();
    await assert.rejects(browserRuntime.invoke("connection_read"));
    assert.equal(reads, 1, "网络失败不能重放业务操作");
    assert.equal(browserRuntime.isRuntimeAvailable(), true, "短暂网络中断保留已挂载工作区");
    server = await listenLocalServer({ ...options, port: Number(new URL(origin).port) });
    const beforeReconnect = handshakes;
    await Promise.all(Array.from({ length: 16 }, () => browserRuntime.connectLocalRuntime()));
    assert.equal(handshakes - beforeReconnect, 1, "并发恢复只发送一次真实会话握手");
    assert.deepEqual(await browserRuntime.invoke("connection_read"), { reads: 2 });
    const changed = accounts.activate({ ...principal, user: { id: "other-account", email: "other@example.com" } });
    const replacement = await nativeFetch(`${origin}/api/local/session`, { method: "POST", headers: { origin, "content-type": "application/json" }, body: JSON.stringify({ ticket: sessions.issueTicket(changed) }) });
    cookie = replacement.headers.get("set-cookie")!.split(";")[0]!;
    await assert.rejects(browserRuntime.connectLocalRuntime(true), /access.account_changed/u);
    assert.equal(browserRuntime.isRuntimeAvailable(), false, "账号变化先关闭原账号工作区");
    await browserRuntime.connectLocalRuntime();
    assert.equal(browserRuntime.unifiedAccountSessionKey(), JSON.stringify(["other-account", changed.epoch]));
    sessions.clear();
    await assert.rejects(browserRuntime.invoke("connection_read"), /web_session_required/u);
    assert.equal(browserRuntime.isRuntimeAvailable(), false, "服务清除会话后必须重新授权");
    assert.equal(reads, 2);
  } finally {
    globalThis.fetch = nativeFetch;
    await server.close();
  }
});
