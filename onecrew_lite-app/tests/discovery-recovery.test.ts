import assert from "node:assert/strict";
import { copyFile, mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { DiscoveryRepository } from "../src/discovery/repository.ts";
import { recoverDiscoveryReads } from "../src/discovery/recovery.ts";

test("真实原schema90跨类型中断恢复保留已计费原领取，不自动重试未知搜索，事务失败共同回滚", async () => {
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/discovery-recovery-", import.meta.url))), path = join(root, "crm.sqlite");
  await copyFile(new URL("./fixtures/discovery-profile-hydration/claimed.sqlite", import.meta.url), path);
  const db = new DatabaseSync(path), repo = new DiscoveryRepository(db);
  try {
    repo.create({ id: "unresolved-search", brief: "找鞋子作者", opencli_version: "1.8.8", criteria: {}, scopes: [{ platform: "youtube", profile_ref: "work", keywords: ["shoe review"] }], target_count: 2, max_calls: 4 }); repo.approve("unresolved-search", 1);
    const search = repo.claimSearch("unresolved-search", 1, "youtube", "work"), before = db.prepare("SELECT id,used_calls FROM discovery_campaigns ORDER BY id").all();
    db.exec("CREATE TRIGGER deny_recovery BEFORE UPDATE ON discovery_platform_runs BEGIN SELECT RAISE(ABORT,'recovery unavailable'); END");
    assert.throws(() => recoverDiscoveryReads(db), /recovery unavailable/);
    assert.equal(db.prepare("SELECT COUNT(*) AS count FROM discovery_platform_runs WHERE status='searching'").get()!.count, 2);
    db.exec("DROP TRIGGER deny_recovery");
    assert.equal(recoverDiscoveryReads(db), 2); assert.equal(recoverDiscoveryReads(db), 0);
    assert.deepEqual(db.prepare("SELECT id,used_calls FROM discovery_campaigns ORDER BY id").all(), before);
    assert.equal(repo.getPlan("profile-hydration").platform_runs[0]!.stop_code, "interrupted_profile_read");
    assert.equal(repo.getPlan("unresolved-search").platform_runs[0]!.stop_code, "interrupted_read");
    assert.throws(() => repo.claimSearch("unresolved-search", 1, "youtube", "work"), /explicit_recovery_required/);
    const explicitlyResumed = repo.claimSearch("unresolved-search", 1, "youtube", "work", true);
    assert.equal(explicitlyResumed.query, search.query); assert.equal(explicitlyResumed.callNumber, search.callNumber + 1);
    assert.equal(db.prepare("SELECT status FROM discovery_profile_reads").get()!.status, "reading");
  } finally { db.close(); }
});
