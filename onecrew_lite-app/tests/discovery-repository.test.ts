import assert from "node:assert/strict";
import { mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { initializeNewCreatorDatabase } from "../src/storage/creator-schema.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { DiscoveryRepository } from "../src/discovery/repository.ts";

test("真实schema90发现搜索领取原子计费，双连接互斥，回执与零结果严格区分", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/discovery-repository-", import.meta.url)));
  const path = join(await createPrivateDirectory(join(directory, "private")), "creator.sqlite");
  await initializeNewCreatorDatabase(path); const db = new DatabaseSync(path), other = new DatabaseSync(path);
  const first = new DiscoveryRepository(db), second = new DiscoveryRepository(other);
  try {
    db.exec("PRAGMA foreign_keys=ON"); other.exec("PRAGMA foreign_keys=ON");
    first.create({ id: "discovery-local", brief: "找鞋子作者", opencli_version: "1.8.8", criteria: {}, scopes: [{ platform: "youtube", profile_ref: "work", keywords: ["shoe review", "sneaker review", "sneaker fit"] }], target_count: 2 });
    assert.throws(() => first.claimSearch("discovery-local", 1, "youtube", "work"));
    first.approve("discovery-local", 1);
    const claim = first.claimSearch("discovery-local", 1, "youtube", "work");
    assert.equal(claim.query, "shoe review"); assert.throws(() => second.claimSearch("discovery-local", 1, "youtube", "work"));
    first.finishSearch(claim, "technical_stop", "opencli_command_failed", []);
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM discovery_search_completions").get()?.n, 0);
    assert.throws(() => first.finishSearch(claim, "completed", null, []));
    const next = second.claimSearch("discovery-local", 1, "youtube", "work"); assert.equal(next.query, "sneaker review");
    second.finishSearch(next, "completed", null, []);
    assert.equal(db.prepare("SELECT used_calls FROM discovery_campaigns").get()?.used_calls, 2);
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM discovery_search_completions").get()?.n, 1);
    const third = first.claimSearch("discovery-local", 1, "youtube", "work");
    assert.equal(third.query, "sneaker fit");
    first.finishSearch(third, "completed", null, []);
    assert.equal("max_calls" in first.getPlan("discovery-local"), false);
    assert.throws(() => first.claimSearch("discovery-local", 1, "youtube", "work"), /no_remaining_keyword/);
    first.adjustTarget("verified-owner", { decisionId: "target-1", planId: "discovery-local", expectedRevision: 1, expectedTargetCount: 2, newTargetCount: 1 });
    second.adjustTarget("verified-owner", { decisionId: "target-1", planId: "discovery-local", expectedRevision: 1, expectedTargetCount: 2, newTargetCount: 1 });
    assert.equal(first.getPlan("discovery-local").target_count, 1);
    assert.equal(first.getPlan("discovery-local").used_calls, 3);
    assert.throws(() => second.adjustTarget("verified-owner", { decisionId: "target-1", planId: "discovery-local", expectedRevision: 1, expectedTargetCount: 2, newTargetCount: 3 }), /idempotency_conflict/);
    first.create({ id: "restart", brief: "找作者", opencli_version: "1.8.8", criteria: { automatic_query_expansion: true }, scopes: [{ platform: "youtube", profile_ref: "work", keywords: ["review"] }], target_count: 2 });
    first.approve("restart", 1); const interrupted = first.claimSearch("restart", 1, "youtube", "work");
    assert.equal(second.recoverInterruptedSearches(), 1);
    assert.throws(() => first.claimSearch("restart", 1, "youtube", "work"), /explicit_recovery_required/);
    const recovery = second.claimSearch("restart", 1, "youtube", "work", true);
    assert.equal(recovery.query, interrupted.query); assert.equal(recovery.callNumber, 2);
    assert.throws(() => first.finishSearch(interrupted, "completed", null, []), /claim_stale/);
    second.finishSearch(recovery, "completed", null, []);
    const expected = [{ platform: "youtube", profile_ref: "work", keywords: ["review"] }], additions = [{ platform: "youtube", profile_ref: "work", keywords: ["author review"] }];
    first.appendQueries("verified-owner", "restart", 1, expected, additions);
    assert.throws(() => second.appendQueries("verified-owner", "restart", 1, expected, additions), /plan_changed/);
    assert.equal(first.claimSearch("restart", 1, "youtube", "work").query, "author review");
  } finally { other.close(); db.close(); }
});

test("三个连续搜索失败持久跳过该平台，继续另一个平台，成功会清零连续失败", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/discovery-search-retries-", import.meta.url)));
  const path = join(directory, "creator.sqlite");
  await initializeNewCreatorDatabase(path);
  const db = new DatabaseSync(path);
  try {
    db.exec("PRAGMA foreign_keys=ON");
    const repo = new DiscoveryRepository(db);
    repo.create({ id: "retry-platform", brief: "寻找 AI Agent 作者", opencli_version: "1.8.8", criteria: {}, target_count: 10, scopes: [
      { platform: "youtube", profile_ref: "work", keywords: ["AI agent", "agent builder", "agent tutorial", "AI workflow"] },
      { platform: "twitter", profile_ref: "work", keywords: ["AI agent"] },
    ] });
    repo.approve("retry-platform", 1);
    for (const keyword of ["AI agent", "agent builder", "agent tutorial"]) {
      const current = new DiscoveryRepository(db);
      assert.deepEqual(current.planStep("retry-platform", 1), { kind: "search", platform: keyword === "AI agent" ? "youtube" : "twitter", profile_ref: "work" });
      const claim = current.claimSearch("retry-platform", 1, "youtube", "work");
      assert.equal(claim.query, keyword);
      current.finishSearch(claim, "technical_stop", "opencli_timeout", []);
    }
    const restarted = new DiscoveryRepository(db);
    assert.equal(restarted.getPlan("retry-platform").platform_runs.find(run => run.platform === "youtube")!.status, "technical_stop");
    assert.equal(restarted.nextSearchKeyword("retry-platform", { platform: "youtube", profile_ref: "work", keywords: ["AI agent", "agent builder", "agent tutorial", "AI workflow"] }), null);
    assert.deepEqual(restarted.planStep("retry-platform", 1), { kind: "search", platform: "twitter", profile_ref: "work" });
    const success = restarted.claimSearch("retry-platform", 1, "twitter", "work");
    restarted.finishSearch(success, "completed", null, []);
    assert.equal(db.prepare("SELECT cursor FROM discovery_platform_runs WHERE platform='twitter'").get()!.cursor, '{"last_query":"AI agent","consecutive_failures":0}');
    assert.equal(restarted.getPlan("retry-platform").used_calls, 4);
  } finally { db.close(); }
});
