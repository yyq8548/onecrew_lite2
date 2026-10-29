import assert from "node:assert/strict";
import test from "node:test";
import { z } from "zod";
import { OperationRegistry } from "../src/application/operation-registry.ts";
import { AccountContextService } from "../src/identity/account-context.ts";

test("batch prevalidation rejects later invalid input without identity checks or effects", () => {
  let effects = 0;
  const registry = new OperationRegistry(new AccountContextService(), async () => { effects++; });
  registry.register("validation_test", z.object({ count: z.number().int().positive() }).strict(), async () => { effects++; });
  registry.validate("validation_test", { count: 1 });
  assert.throws(() => registry.validate("validation_test", { count: -1 }));
  assert.throws(() => registry.validate("validation_test", { count: 1, unexpected: true }));
  assert.throws(() => registry.validate("missing", {}), { code: "operation.not_found" });
  assert.equal(effects, 0);
});

test("identity refresh completes before session epoch validation and operation execution", async () => {
  const accounts = new AccountContextService();
  const principal = { user: { id: "account", email: "user@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" as const };
  const before = accounts.activate(principal);
  let release!: () => void;
  const verification = new Promise<void>(resolve => { release = resolve; });
  const registry = new OperationRegistry(accounts, () => verification);
  let invoked = false;
  registry.register("local.identity.read", z.object({}).strict(), async (_, context) => {
    invoked = true;
    return context.principal.user.id;
  });
  const request = registry.execute("local.identity.read", {}, "account", before.epoch);
  assert.equal(invoked, false);
  const after = accounts.activate(principal);
  release();
  await assert.rejects(request, { code: "access.account_changed" });
  assert.equal(invoked, false);
  assert.equal(await registry.execute("local.identity.read", {}, "account", after.epoch), "account");
});

test("one authorized request reuses its identity while later and detached operations refresh", async () => {
  const accounts = new AccountContextService();
  const context = accounts.activate({ user: { id: "account", email: "user@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" });
  let refreshes = 0;
  const registry = new OperationRegistry(accounts, async () => { refreshes++; });
  registry.register("local.identity.read", z.strictObject({}), async (_, current) => current.principal.user.id);
  let release!: () => void;
  const pending = new Promise<void>(resolve => { release = resolve; });
  let detached!: Promise<unknown>;
  await registry.withAuthorizedAccount(context, async () => {
    assert.deepEqual(await Promise.all([registry.execute("local.identity.read", {}, "account", context.epoch), registry.execute("local.identity.read", {}, "account", context.epoch)]), ["account", "account"]);
    detached = pending.then(() => registry.execute("local.identity.read", {}, "account", context.epoch));
    assert.equal(refreshes, 0);
  });
  release();
  await detached;
  assert.equal(refreshes, 1);
  await registry.execute("local.identity.read", {}, "account", context.epoch);
  assert.equal(refreshes, 2);
  await registry.withAuthorizedAccount(context, async () => {
    accounts.activate(context.principal);
    await assert.rejects(registry.execute("local.identity.read", {}, "account", context.epoch), { code: "access.account_changed" });
  });
});
