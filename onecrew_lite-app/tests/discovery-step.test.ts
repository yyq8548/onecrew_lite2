import assert from "node:assert/strict";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { DiscoveryRepository } from "../src/discovery/repository.ts";
import { readFileSync } from "node:fs";
import { discoveryLanguageClaim } from "../src/discovery/language-contracts.ts";

const cases = [
  ["discovery-profile-hydration/before.sqlite", "profile-hydration", "read_profile"],
  ["discovery-profile-hydration/claimed.sqlite", "profile-hydration", "busy"],
  ["discovery-profile-hydration/after.sqlite", "profile-hydration", "read_content"],
  ["discovery-video-hydration/before.sqlite", "video-hydration", "read_video"],
  ["discovery-video-hydration/after.sqlite", "video-hydration", "needs_evidence"],
  ["discovery-transcript-hydration/before.sqlite", "transcript", "read_transcript"],
  ["discovery-post-author/before.sqlite", "identity", "read_identity"],
  ["discovery-content-hydration/bilibili-before.sqlite", "content", "read_content"],
  ["discovery-content-hydration/twitter-before.sqlite", "x-content", "read_content"],
  ["discovery-facebook-content/before.sqlite", "facebook-native-identity", "read_content"],
  ["discovery-source-author/before.sqlite", "douyin-source-flow", "read_source_author"],
] as const;
test("完整下一步优先级从原Rust十一份真实快照选择原批准任务，不触发提供方或领取预算", () => {
  for (const [fixture, campaign, kind] of cases) {
    using database = new DatabaseSync(fileURLToPath(new URL(`./fixtures/${fixture}`, import.meta.url)), { readOnly: true });
    const repository = new DiscoveryRepository(database), before = repository.getPlan(campaign).used_calls;
    assert.equal(repository.planStep(campaign, 1).kind, kind, fixture); assert.equal(repository.getPlan(campaign).used_calls, before);
    assert.throws(() => repository.planStep(campaign, 2), /approval_changed/);
  }
});

test("待核实恢复从真实来源绑定去重，遵守修订和排除平台且不领取预算", () => {
  const claim = discoveryLanguageClaim.parse(JSON.parse(readFileSync(new URL("./fixtures/discovery-language/tiktok.json", import.meta.url), "utf8")).claim);
  using database = new DatabaseSync(fileURLToPath(new URL("./fixtures/discovery-language/tiktok-before.sqlite", import.meta.url)), { readOnly: true });
  const repository = new DiscoveryRepository(database), before = repository.getPlan(claim.campaign_id).used_calls;
  assert.deepEqual(repository.pendingScoreHits(claim.campaign_id, claim.revision, []), [claim.search_hit_id]);
  assert.deepEqual(repository.pendingScoreHits(claim.campaign_id, claim.revision, ["tiktok"]), []);
  assert.deepEqual(repository.pendingScoreHits(claim.campaign_id, claim.revision + 1, []), []);
  assert.equal(repository.getPlan(claim.campaign_id).used_calls, before);
});
