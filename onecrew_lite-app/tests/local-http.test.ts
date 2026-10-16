import assert from "node:assert/strict";
import { randomBytes, randomUUID } from "node:crypto";
import test from "node:test";
import { connect } from "node:net";
import { once } from "node:events";
import { OperationRegistry } from "../src/application/operation-registry.ts";
import { AccountContextService } from "../src/identity/account-context.ts";
import { listenLocalServer } from "../src/service/http.ts";

test("shutdown closes an incomplete HTTP connection before request dispatch", { timeout: 5_000 }, async () => {
  const server = await listenLocalServer({ port: 0, instanceId: randomUUID(), controlToken: randomBytes(32).toString("hex"), operations: new OperationRegistry(new AccountContextService()) });
  const socket = connect(Number(new URL(server.origin).port), "127.0.0.1");
  await once(socket, "connect");
  socket.write("GET / HTTP/1.1\r\nHost: 127.0.0.1\r\n");
  await new Promise(resolve => setTimeout(resolve, 30));
  const closed = once(socket, "close");
  const stopping = server.close();
  try {
    await Promise.race([stopping, new Promise<never>((_, reject) => {
      const timeout = setTimeout(() => reject(new Error("Browser preconnection blocked shutdown")), 1_000);
      timeout.unref();
    })]);
    await closed;
  } finally {
    socket.destroy();
    await stopping;
  }
});

test("real loopback HTTP rejects hostile origins, unauthenticated clients and retired sends", async () => {
  const accounts = new AccountContextService();
  const context = accounts.activate({ user: { id: "account", email: "user@example.com" },
    tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" });
  const operations = new OperationRegistry(accounts);
  const token = randomBytes(32).toString("hex");
  const instanceId = randomUUID();
  const server = await listenLocalServer({ port: 0, instanceId, controlToken: token, operations });
  const headers = { authorization: `Bearer ${token}`, "content-type": "application/json",
    "x-onecrew-account-id": "account", "x-onecrew-auth-epoch": context.epoch };
  try {
    assert.equal((await fetch(`${server.origin}/_local/status`)).status, 401);
    assert.equal((await fetch(`${server.origin}/_local/status`, { headers: { ...headers, origin: "https://hostile.example" } })).status, 403);
    const status = await fetch(`${server.origin}/_local/status`, { headers });
    assert.equal(status.status, 200);
    assert.equal(status.headers.get("cache-control"), "no-store");
    assert.equal((await status.json()).instance_id, instanceId);
    const retired = await fetch(`${server.origin}/api/operations/x.message.send`, { method: "POST", headers, body: "{}" });
    assert.equal(retired.status, 410);
    assert.deepEqual(await retired.json(), { error: { code: "operation.retired" } });
    const oldCommand = await fetch(`${server.origin}/api/commands/onecrew_email_draft_send`, { method: "POST", headers, body: "{}" });
    assert.equal(oldCommand.status, 410);
    const malformed = await fetch(`${server.origin}/api/operations/x.profile.get`, { method: "POST", headers, body: "{" });
    assert.equal(malformed.status, 400);
    const tooLarge = await fetch(`${server.origin}/api/operations/x.profile.get`, { method: "POST", headers, body: JSON.stringify("a".repeat(1024 * 1024)) });
    assert.equal(tooLarge.status, 413);
    await assert.rejects(listenLocalServer({ port: Number(new URL(server.origin).port), instanceId: randomUUID(), controlToken: token, operations }), { code: "EADDRINUSE" });
    accounts.logout();
    const stale = await fetch(`${server.origin}/api/operations/x.message.send`, { method: "POST", headers, body: "{}" });
    assert.equal(stale.status, 409);
  } finally {
    await server.close();
  }
});
