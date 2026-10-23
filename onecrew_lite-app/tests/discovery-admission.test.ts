import assert from "node:assert/strict";
import { test } from "node:test";
import { AccountContextService } from "../src/identity/account-context.ts";
import { DiscoveryAdmissionGate } from "../src/discovery/admission-gate.ts";

test("search cancels a background admission but waits for its transaction to drain", async () => {
  const accounts = new AccountContextService();
  const context = accounts.activate({ user: { id: "owner", email: "owner@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" });
  const gate = new DiscoveryAdmissionGate(accounts), cancel = new AbortController();
  const upload = await gate.acquire(context, context.signal, cancel);
  let started = false;
  const search = gate.acquire(context).then(lease => { started = true; return lease; });
  assert.equal(cancel.signal.aborted, true);
  await Promise.resolve(); assert.equal(started, false);
  upload.release(); const lease = await search; lease.assertCurrent(); lease.release();
  accounts.assertCurrent(context);
});

test("共享admission持有期间互斥，排队取消不抢占旧事务，账号切换后原lease仍需明确排空释放", async () => {
  const accounts = new AccountContextService(), principal = { user: { id: "local-rule-owner", email: "owner@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" as const };
  const context = accounts.activate(principal), gate = new DiscoveryAdmissionGate(accounts), upload = await gate.acquire(context);
  const cancel = new AbortController(); let discoveryStarted = false;
  const cancelled = gate.acquire(context, cancel.signal); const cancelledCheck = assert.rejects(cancelled, /local_stop/);
  cancel.abort(new Error("local_stop")); await cancelledCheck;
  const discovery = gate.acquire(context).then(lease => { discoveryStarted = true; return lease; });
  await Promise.resolve(); assert.equal(discoveryStarted, false);
  upload.release(); const searching = await discovery; assert.equal(discoveryStarted, true); searching.assertCurrent();
  const next = accounts.activate({ ...principal, user: { id: "next-local-owner", email: "next@example.com" } });
  assert.throws(() => searching.assertCurrent(), /account changed/i);
  let nextStarted = false; const nextUpload = gate.acquire(next).then(lease => { nextStarted = true; return lease; });
  await Promise.resolve(); assert.equal(nextStarted, false);
  searching.release(); searching.release(); const current = await nextUpload; assert.equal(nextStarted, true); current.assertCurrent(); current.release();
  await assert.rejects(gate.acquire(context), /account changed/i);
});
