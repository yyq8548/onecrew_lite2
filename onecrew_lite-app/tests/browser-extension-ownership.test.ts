import test from "node:test";
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { mkdir, readdir, rm } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { AccountContextService } from "../src/identity/account-context.ts";
import { AccountDatabases } from "../src/identity/account-databases.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { ManagedBrowserOwnership } from "../src/platforms/browser/extension-ownership.ts";
import { VerifiedManagedExtensionContext } from "../src/platforms/browser/extension-challenge.ts";

test("real account storage rejects unissued browser contexts before creating keys or ownership manifests", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/", import.meta.url)); await mkdir(scratch, { recursive: true });
  const root = await createPrivateDirectory(join(scratch, `bo-${randomUUID().slice(0, 12)}`)), lease = await acquireInstanceLock(root);
  const accounts = new AccountContextService(), databases = new AccountDatabases(lease, accounts), authority = new ManagedBrowserOwnership(accounts, databases);
  try {
    const context = accounts.activate({ user: { id: "browser-owner", email: "local@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" });
    await databases.ensureNewAccount(context);
    const account = await databases.accountDirectory(context), before = await readdir(account.directory);
    assert.throws(() => Reflect.construct(VerifiedManagedExtensionContext, [Symbol(), "context", {}, context.signal]), /extension_challenge_required/u);
    await assert.rejects(authority.register(context, Object.create(VerifiedManagedExtensionContext.prototype)), TypeError);
    assert.deepEqual(await readdir(account.directory), before);
    await assert.rejects(authority.verify({ accountId: "other", tenantId: "tenant", baseId: "base", deviceId: "device", contextId: "context", profileDirectory: account.directory, proof: `v1.${"A".repeat(43)}` }), /owner_mismatch/u);
    accounts.logout();
    await assert.rejects(authority.register(context, Object.create(VerifiedManagedExtensionContext.prototype)), { code: "access.account_changed" });
    assert.deepEqual(await readdir(account.directory), before);
  } finally { databases.close(); await lease.close(); await rm(root, { recursive: true }); }
});
