import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { AccountDatabases } from "../src/identity/account-databases.ts";
import { AccountContextService } from "../src/identity/account-context.ts";
import { OperationRegistry } from "../src/application/operation-registry.ts";
import { registerGmailBulkOperations } from "../src/mail/gmail-bulk-account-binding.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { DatabaseSync } from "node:sqlite";

test("only committed confirm and resume wake bulk processing; drafts, invalid controls and stale accounts do not", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/gmail-control-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "control-"));
  const lease = await acquireInstanceLock(await createPrivateDirectory(join(root, "private")));
  const accounts = new AccountContextService();
  const context = accounts.activate({ user: { id: "owner", email: "owner@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" });
  const databases = new AccountDatabases(lease, accounts);
  try {
    await databases.createNewAccount(context);
    const directory = await databases.accountDirectory(context);
    using database = new DatabaseSync(join(directory.directory, "creator.sqlite"));
    database.prepare("INSERT INTO mailbox_refs VALUES ('box','gmail','Gmail','sender@example.com',NULL,'[\"send\",\"sent_readback\"]',1,NULL,1,1)").run();
    const registry = new OperationRegistry(accounts);
    let wakes = 0;
    registerGmailBulkOperations(registry, databases.bulkResolver(), accounts, () => { wakes++; });
    const execute = (action: string, id: string) => registry.execute("onecrew_gmail_bulk_control", { input: { id, action } }, context.principal.user.id, context.epoch);
    const batch = await registry.execute("onecrew_gmail_bulk_prepare", { input: { mailbox_ref_id: "box", subject: "subject", body: "body", recipients: [{ email: "recipient@example.com", source: "Creator" }] } }, "owner", context.epoch) as { id: string };
    assert.equal(wakes, 0);
    await assert.rejects(execute("resume", batch.id));
    assert.equal(wakes, 0);
    await execute("confirm", batch.id);
    assert.equal(wakes, 1);
    const repository = (await databases.bulkResolver().resolve(context)).repository;
    assert.equal(repository.get(batch.id).state, "running");
    await execute("pause", batch.id);
    assert.equal(wakes, 1);
    await execute("resume", batch.id);
    assert.equal(wakes, 2);
    await execute("cancel", batch.id);
    assert.equal(wakes, 2);
    accounts.logout();
    await assert.rejects(execute("resume", batch.id), { code: "access.account_changed" });
    assert.equal(wakes, 2);
  } finally { databases.close(); await lease.close(); await rm(root, { recursive: true }); }
});
