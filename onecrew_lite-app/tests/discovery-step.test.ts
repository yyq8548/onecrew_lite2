import assert from "node:assert/strict";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { DiscoveryRepository } from "../src/discovery/repository.ts";
import { readFileSync } from "node:fs";
import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { discoveryLanguageClaim } from "../src/discovery/language-contracts.ts";
import { backupLegacyDatabase } from "../src/storage/legacy-sqlite.ts";
import { upgradeCreatorStagingCopy } from "../src/storage/creator-upgrade.ts";

async function withCurrentFixture(fixture: string, check: (database: DatabaseSync) => void) {
  await mkdir(fileURLToPath(new URL("../../.scratch/", import.meta.url)), { recursive: true });
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/discovery-step-", import.meta.url)));
  const sourcePath = fileURLToPath(new URL(`./fixtures/${fixture}`, import.meta.url));
  const stagingPath = join(root, "staging", "creator.sqlite");
  try {
    await backupLegacyDatabase(sourcePath, stagingPath, "creator");
    assert.deepEqual(await upgradeCreatorStagingCopy({ sourcePath, stagingPath }), { fromVersion: 90, toVersion: 96 });
    using database = new DatabaseSync(stagingPath, { readOnly: true });
    check(database);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

const cases = [
  ["discovery-video-hydration/before.sqlite", "video-hydration", "read_video"],
  ["discovery-video-hydration/after.sqlite", "video-hydration", "needs_evidence"],
  ["discovery-transcript-hydration/before.sqlite", "transcript", "needs_evidence"],
  ["discovery-post-author/before.sqlite", "identity", "read_identity"],
  ["discovery-content-hydration/bilibili-before.sqlite", "content", "read_content"],
  ["discovery-content-hydration/twitter-before.sqlite", "x-content", "read_content"],
  ["discovery-facebook-content/before.sqlite", "facebook-native-identity", "read_content"],
  ["discovery-source-author/before.sqlite", "douyin-source-flow", "read_source_author"],
] as const;
test("schema90八份受支持平台真实快照升级后按当前规则选择原批准任务，不触发提供方或领取预算", async t => {
  for (const [fixture, campaign, kind] of cases) {
    await t.test(fixture, () => withCurrentFixture(fixture, database => {
      const repository = new DiscoveryRepository(database), before = repository.getPlan(campaign).used_calls;
      assert.equal(repository.planStep(campaign, 1).kind, kind, fixture); assert.equal(repository.getPlan(campaign).used_calls, before);
      const current = repository.planStep(campaign, 1);
      if (["read_video", "read_identity", "read_source_author"].includes(kind) && "search_hit_id" in current) {
        const candidates = repository.listCandidates({ campaign_id: campaign, after_id: null, limit: 100 });
        const next = repository.planStep(campaign, 1, [], [], { heldQueryHits: [current.search_hit_id] });
        assert.ok(!("search_hit_id" in next) || next.search_hit_id !== current.search_hit_id, "暂缓的未读命中不能阻塞其他查询");
        assert.equal(repository.getPlan(campaign).used_calls, before);
        assert.deepEqual(repository.listCandidates({ campaign_id: campaign, after_id: null, limit: 100 }), candidates);
      }
      assert.throws(() => repository.planStep(campaign, 2), /approval_changed/);
    }));
  }
});

test("已移除平台的真实历史快照仍可读取任务和候选，调度不再读取且预算不变", async t => {
  for (const stage of ["before", "claimed", "after"]) {
    const fixture = `discovery-profile-hydration/${stage}.sqlite`;
    await t.test(fixture, () => withCurrentFixture(fixture, database => {
      const repository = new DiscoveryRepository(database), campaign = "profile-hydration", before = repository.getPlan(campaign);
      const candidates = repository.listCandidates({ campaign_id: campaign, after_id: null, limit: 100 });
      assert.equal(before.id, campaign);
      assert.equal(candidates.length, Number(database.prepare("SELECT COUNT(*) AS count FROM discovery_candidates WHERE discovery_campaign_id=?").get(campaign)!.count));
      const step = repository.planStep(campaign, before.revision);
      assert.equal(step.kind.startsWith("read_"), false, fixture);
      assert.notEqual(step.kind, "search", fixture);
      assert.deepEqual(repository.getPlan(campaign), before);
      assert.deepEqual(repository.listCandidates({ campaign_id: campaign, after_id: null, limit: 100 }), candidates);
      assert.throws(() => repository.planStep(campaign, before.revision + 1), /approval_changed/);
    }));
  }
});

test("待核实恢复从真实来源绑定去重，遵守修订和排除平台且不领取预算", async () => {
  const claim = discoveryLanguageClaim.parse(JSON.parse(readFileSync(new URL("./fixtures/discovery-language/tiktok.json", import.meta.url), "utf8")).claim);
  await withCurrentFixture("discovery-language/tiktok-before.sqlite", database => {
    const repository = new DiscoveryRepository(database), before = repository.getPlan(claim.campaign_id).used_calls;
    assert.deepEqual(repository.pendingScoreHits(claim.campaign_id, claim.revision, []), [claim.search_hit_id]);
    assert.deepEqual(repository.pendingScoreHits(claim.campaign_id, claim.revision, ["tiktok"]), []);
    assert.deepEqual(repository.pendingScoreHits(claim.campaign_id, claim.revision + 1, []), []);
    assert.equal(repository.getPlan(claim.campaign_id).used_calls, before);
  });
});
