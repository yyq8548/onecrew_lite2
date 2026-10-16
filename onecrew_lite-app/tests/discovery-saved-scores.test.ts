import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { basename, dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { AccountContextService } from "../src/identity/account-context.ts";
import { AccountDatabases } from "../src/identity/account-databases.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { resolveDiscoveryScoreDirectory, SavedDiscoveryScores } from "../src/discovery/saved-scores.ts";

test("真实账号私有目录只恢复未启动评分，串行写入不丢失且账号/修订隔离", async () => {
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/discovery-score-queue-", import.meta.url))), directory = await createPrivateDirectory(join(root, "private"));
  const lease = await acquireInstanceLock(directory), accounts = new AccountContextService(), databases = new AccountDatabases(lease, accounts);
  const principal = { user: { id: "account-a", email: "a@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" as const };
  let closed = false;
  try {
    const context = accounts.activate(principal); await databases.createNewAccount(context); const owned = await resolveDiscoveryScoreDirectory(databases, context);
    const queue = new SavedDiscoveryScores(owned, "../campaign", 1), same = new SavedDiscoveryScores(owned, "../campaign", 1);
    assert.equal(dirname(queue.path), join(owned.directory, "discovery-score-queue"));
    assert.equal(basename(queue.path), `${createHash("sha256").update(JSON.stringify(["account-a", "../campaign", 1])).digest("hex")}.json`);
    await Promise.all([queue.add({ hit: "first", deferred_platforms: ["youtube"] }), same.add({ hit: "second", deferred_platforms: ["bilibili"] })]);
    assert.deepEqual((await queue.load()).map(row => row.hit), ["first", "second"]);
    await queue.remove("first"); assert.deepEqual(await same.load(), [{ hit: "second", deferred_platforms: ["bilibili"] }]);
    assert.deepEqual(await new SavedDiscoveryScores(owned, "../campaign", 2).load(), []);
    const next = accounts.activate({ ...principal, user: { id: "account-b", email: "b@example.com" } }); await databases.createNewAccount(next);
    assert.deepEqual(await new SavedDiscoveryScores(await resolveDiscoveryScoreDirectory(databases, next), "../campaign", 1).load(), []);
    for (const hit of ["third", "fourth", "fifth", "sixth"]) await queue.add({ hit, deferred_platforms: [] });
    const before = await readFile(queue.path); await assert.rejects(queue.add({ hit: "overflow", deferred_platforms: [] }), /exceeds_capacity/);
    assert.deepEqual(await readFile(queue.path), before);
    await writeFile(queue.path, "invalid"); const malformed = await readFile(queue.path); await assert.rejects(queue.load());
    assert.deepEqual(await readFile(queue.path), malformed);
    databases.close(); closed = true; await assert.rejects(queue.load(), /closed/);
  } finally { if (!closed) databases.close(); await lease.close(); }
});
