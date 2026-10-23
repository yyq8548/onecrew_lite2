import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { AccountDatabases } from "../src/identity/account-databases.ts";
import { AccountContextService } from "../src/identity/account-context.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";

test("real private account databases preserve ownership, reuse connections and reject stale contexts", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/account-database-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "accounts-"));
  const directory = await createPrivateDirectory(join(root, "private"));
  const lease = await acquireInstanceLock(directory);
  const accounts = new AccountContextService();
  const databases = new AccountDatabases(lease, accounts);
  const principal = { user: { id: "account-a", email: "a@example.com" }, tenantId: "tenant-a", baseId: "base-a", deviceId: "device-a", source: "access" as const };
  try {
    const first = accounts.activate(principal);
    await assert.rejects(databases.resolve(first));
    const [a, b] = await Promise.all([databases.ensureNewAccount(first), databases.ensureNewAccount(first)]);
    assert.equal(a, b);
    assert.equal(a.owner.accountId, "account-a");
    assert.deepEqual(a.bulk.list(), []);
    assert.deepEqual(a.history.mailboxes(), []);
    const discovery = await databases.discoveryResolver().resolve(first);
    assert.equal(discovery.repository, a.discovery);
    discovery.assertOwned();
    const second = accounts.activate({ ...principal, user: { id: "account-b", email: "b@example.com" }, tenantId: "tenant-b", baseId: "base-b" });
    await assert.rejects(databases.resolve(first), { code: "access.account_changed" });
    discovery.assertOwned();
    await assert.rejects(databases.discoveryResolver().resolve(first), { code: "access.account_changed" });
    await assert.rejects(databases.resolve(second));
    const other = await databases.ensureNewAccount(second);
    assert.notEqual(a.bulk, other.bulk);
    assert.equal(other.owner.accountId, "account-b");
    assert.deepEqual(a.bulk.list(), []);
    const again = accounts.activate(principal);
    assert.equal(await databases.resolve(again), a);
    assert.equal(await databases.ensureNewAccount(again), a);
    await assert.rejects(databases.createNewAccount(again));
    const account = await databases.accountDirectory(again), manifestPath = join(account.directory, "account.json"), bytes = await readFile(manifestPath);
    await writeFile(manifestPath, Buffer.concat([bytes, Buffer.from(" ")]));
    assert.throws(() => discovery.assertOwned(), /ownership changed/);
    await writeFile(manifestPath, bytes);
    discovery.assertOwned();
    assert.throws(() => new AccountDatabases(lease, accounts, root), /belong to the current lease/);
  } finally {
    databases.close();
    await lease.close();
    await rm(root, { recursive: true });
  }
});
