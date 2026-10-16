import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { randomUUID } from "node:crypto";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { prepareTableSyncStaging, verifyTableSyncStaging } from "../src/migration/table-sync-staging.ts";
import { tableSyncKey, tableSyncStoreSchema } from "../src/sync/table-sync-store.ts";
import { parseApplicationJson, stringifyApplicationJson } from "../src/contracts/application-json.ts";

test("table sync staging preserves complete history and partitions actual u64 state without claiming other users", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/", import.meta.url)); await mkdir(scratch, { recursive: true });
  const root = await createPrivateDirectory(join(scratch, `table-sync-staging-${randomUUID()}`));
  const old = await createPrivateDirectory(join(root, "old")), target = await createPrivateDirectory(join(root, "target"));
  const lease = await acquireInstanceLock(target);
  try {
    const generation = await createPrivateDirectory(join(target, "generation-test")), account = await createPrivateDirectory(join(generation, "account-test"));
    const table = (user: string) => ({ campaign_id: "campaign", user_id: user, enabled: true, next_check: 18446744073709551615n, committed_hash: "old", previous_version: randomUUID(), sequence: 9007199254740993n, status: "unknown", error: null, pending: null });
    const source = Buffer.from(stringifyApplicationJson({ local_source_id: randomUUID(), tables: { [tableSyncKey("selected", "campaign")]: table("selected"), [tableSyncKey("history", "campaign")]: table("history") }, consent_operation: { original: "保留历史授权状态" }, consent_user: "history" }));
    const sourcePath = join(old, "dynamic-table-sync.json"); await writeFile(sourcePath, source, { flag: "wx", mode: 0o600 });
    const receipt = await prepareTableSyncStaging(lease, { sourceDirectory: old, generationDirectory: generation, accountDirectory: account, accountId: "selected" });
    assert.deepEqual(receipt.otherUserIds, ["history"]);
    assert.deepEqual(await readFile(sourcePath), source); assert.deepEqual(await readFile(receipt.backupPath!), source);
    const selected = tableSyncStoreSchema.parse(parseApplicationJson(await readFile(receipt.selectedPath!)));
    assert.deepEqual(Object.keys(selected.tables), [tableSyncKey("selected", "campaign")]);
    assert.equal(selected.consent_operation, null); assert.equal(selected.consent_user, null);
    assert.equal(BigInt(selected.tables[tableSyncKey("selected", "campaign")]!.sequence), 9007199254740993n);
    assert.equal(BigInt(selected.tables[tableSyncKey("selected", "campaign")]!.next_check), 18446744073709551615n);
    assert.equal(selected.tables[tableSyncKey("selected", "campaign")]!.status, "unknown");
    await verifyTableSyncStaging(lease, receipt, generation, account, "selected");
    await assert.rejects(verifyTableSyncStaging(lease, receipt, generation, account, "history"), /owner_changed/u);
    await writeFile(sourcePath, source);
    await assert.rejects(verifyTableSyncStaging(lease, receipt, generation, account, "selected"), /source_changed/u);
    const absentSource = await createPrivateDirectory(join(root, "absent-source"));
    const absent = await prepareTableSyncStaging(lease, { sourceDirectory: absentSource, generationDirectory: generation, accountDirectory: account, accountId: "selected" });
    assert.equal(absent.source, null);
    await writeFile(join(absentSource, "dynamic-table-sync.json"), "{}", { flag: "wx", mode: 0o600 });
    await assert.rejects(verifyTableSyncStaging(lease, absent, generation, account, "selected"), /source_changed/u);
  } finally { await lease.close(); await rm(root, { recursive: true }); }
});
