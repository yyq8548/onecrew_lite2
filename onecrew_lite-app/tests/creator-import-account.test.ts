import assert from "node:assert/strict";
import { mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { OperationRegistry } from "../src/application/operation-registry.ts";
import { registerCreatorOperations } from "../src/creators/operations.ts";
import { AccountDatabases } from "../src/identity/account-databases.ts";
import { AccountContextService } from "../src/identity/account-context.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { buildBundle } from "../src/creators/transfer-contracts.ts";
import { encodeWorkbook } from "../src/creators/workbook.ts";

test("真实账号上传预览/确认/导出隔离，写后通知一次，批次不暴露无审核执行入口", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/creator-import-account-", import.meta.url))), lease = await acquireInstanceLock(await createPrivateDirectory(join(directory, "private")));
  const accounts = new AccountContextService(), databases = new AccountDatabases(lease, accounts);
  try {
    const a = { user: { id: "import-a", email: "a@example.com" }, tenantId: "tenant-a", baseId: "base-a", deviceId: "device", source: "access" as const }, b = { user: { id: "import-b", email: "b@example.com" }, tenantId: "tenant-b", baseId: "base-b", deviceId: "device", source: "access" as const };
    await databases.createNewAccount(accounts.activate(a)); await databases.createNewAccount(accounts.activate(b)); const context = accounts.activate(a), registry = new OperationRegistry(accounts), changes: string[][] = [];
    registerCreatorOperations(registry, databases.creatorsResolver(), accounts, (_context, scopes) => changes.push([...scopes]), async () => directory);
    assert.ok(!registry.names().includes("onecrew.creator.batch.create"));
    const bundle = buildBundle([{ semantic_sha256: "", record_key: "", display_name: "Uploaded", tags: [], score: null, follower_counts: {}, pinned: false, relationship: { stage: "prospect" }, accounts: [], contact_routes: [] }]), bytes = await encodeWorkbook(bundle);
    await assert.rejects(registry.execute("onecrew_creator_import_file_prepare", { file_name: "../file.xlsx", bytes }, a.user.id, context.epoch), /xlsx file name/u);
    const preview = await registry.execute("onecrew_creator_import_file_prepare", { file_name: "样例.xlsx", bytes }, a.user.id, context.epoch) as { status: string; file_name: string; operation_id: string; preview_rows: unknown[] };
    assert.equal(preview.status, "confirmation_required"); assert.equal(preview.preview_rows.length, 1); assert.equal(preview.file_name, "样例.xlsx"); assert.equal(changes.length, 0);
    const input = { input: { operation_id: preview.operation_id } }, switched = accounts.activate(b);
    await assert.rejects(registry.execute("onecrew_creator_import_confirm", input, b.user.id, switched.epoch), /operation not found/u);
    await assert.rejects(registry.execute("onecrew_creator_import_confirm", input, a.user.id, context.epoch), { code: "access.account_changed" });
    const again = accounts.activate(a);
    const results = await Promise.all([registry.execute("onecrew_creator_import_confirm", input, a.user.id, again.epoch), registry.execute("onecrew_creator_import_confirm", input, a.user.id, again.epoch)]);
    assert.deepEqual(results[0], results[1]); assert.deepEqual(changes, [["creators"]]);
    const exported = await registry.execute("onecrew_creator_export", { input: { format: "xlsx" } }, a.user.id, again.epoch) as { destination: string; file_name: string }; assert.equal(exported.destination, "system_desktop"); assert.ok(exported.file_name.endsWith(".xlsx")); assert.equal(changes.length, 1);
    await assert.rejects(registry.execute("onecrew.creator.export", { format: "xlsx", directory }, a.user.id, again.epoch));
  } finally { databases.close(); await lease.close(); }
});
