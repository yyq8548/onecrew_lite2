import assert from "node:assert/strict";
import test from "node:test";
import { DomainChanges } from "../src/application/domain-changes.ts";
import { AccountContextService } from "../src/identity/account-context.ts";

test("account change streams preserve wire hints and close on switch, cancellation and lag", async () => {
  const accounts = new AccountContextService();
  const principal = { user: { id: "account", email: "user@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" as const };
  const first = accounts.activate(principal);
  const changes = new DomainChanges(accounts);
  const abort = new AbortController();
  const reader = changes.subscribe(first, abort.signal).getReader();
  assert.deepEqual((await reader.read()).value, { event: "runtime://changed", payload: { sequence: 0, scopes: ["all"] } });
  changes.publish(first, ["creators", "creators"]);
  assert.deepEqual((await reader.read()).value?.payload, { sequence: 1, scopes: ["creators"] });
  const second = accounts.activate(principal);
  assert.equal((await reader.read()).done, true);
  assert.throws(() => changes.publish(first, ["creators"]), { code: "access.account_changed" });
  const slow = changes.subscribe(second, abort.signal).getReader();
  for (let index = 0; index < 256; index++) changes.publish(second, ["creators"]);
  await assert.rejects(slow.read(), /lagged/);
  const resumed = changes.subscribe(second, abort.signal).getReader();
  assert.equal((await resumed.read()).value?.payload.sequence, 0);
  abort.abort();
  assert.equal((await resumed.read()).done, true);
  changes.close();
});
