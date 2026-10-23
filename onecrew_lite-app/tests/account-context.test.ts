import assert from "node:assert/strict";
import test from "node:test";
import { AccountChangedError, AccountContextService } from "../src/identity/account-context.ts";

test("account switching invalidates old reads and MCP sessions while preserving receipt ownership", () => {
  const service = new AccountContextService();
  const first = service.activate({
    user: { id: "first", email: "first@example.com" },
    tenantId: "tenant-first", baseId: "base-first", deviceId: "device", source: "access",
  });
  const owner = service.receiptOwner(first);
  const second = service.activate({
    user: { id: "second", email: "second@example.com" },
    tenantId: "tenant-second", baseId: "base-second", deviceId: "device", source: "access",
  });
  assert.equal(first.signal.aborted, true);
  assert.throws(() => service.assertCurrent(first), AccountChangedError);
  assert.throws(() => service.forSession("first", first.epoch), AccountChangedError);
  assert.equal(service.forSession("second", second.epoch), second);
  assert.deepEqual(owner, { accountId: "first", tenantId: "tenant-first", epoch: first.epoch });
  assert.throws(() => service.assertCurrent({ ...second }), AccountChangedError);
  service.logout();
  assert.equal(second.signal.aborted, true);
  assert.throws(() => service.current(), /verified user principal/);
});

test("reactivating the same account still invalidates prior authorization epoch", () => {
  const service = new AccountContextService();
  const principal = {
    user: { id: "user", email: "user@example.com" }, tenantId: "tenant", baseId: "base",
    deviceId: "device", source: "access" as const,
  };
  const first = service.activate(principal);
  const second = service.activate(principal);
  assert.notEqual(first.epoch, second.epoch);
  assert.throws(() => service.forSession("user", first.epoch), AccountChangedError);
  principal.user.id = "changed-outside";
  assert.equal(second.principal.user.id, "user");
  assert.equal(Object.isFrozen(second.principal.user), true);
});
