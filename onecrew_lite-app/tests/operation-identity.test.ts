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
