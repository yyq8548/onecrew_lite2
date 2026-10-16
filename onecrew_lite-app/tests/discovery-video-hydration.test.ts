import assert from "node:assert/strict";
import { copyFile, mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { DiscoveryVideoRepository, type DiscoveryVideoCandidate } from "../src/discovery/video-repository.ts";

const source = new URL("./fixtures/discovery-video-hydration/", import.meta.url);
function originalCandidate(db: DatabaseSync): DiscoveryVideoCandidate {
  const row = db.prepare("SELECT * FROM discovery_candidates").get()!, evidence = parseApplicationJson(String(row.evidence_json)) as Record<string, unknown>; delete evidence.assessment;
  return { campaign_id: String(row.discovery_campaign_id), expected_revision: Number(row.criteria_revision), platform: "youtube", source_profile_ref: String(row.source_profile_ref), platform_account_id: String(row.platform_account_id), display_name: String(row.display_name), profile_url: String(row.profile_url), evidence };
}
async function copiedBefore(prefix: string) {
  const root = await mkdtemp(fileURLToPath(new URL(`../../.scratch/${prefix}-`, import.meta.url))), path = join(root, "crm.sqlite"); await copyFile(new URL("before.sqlite", source), path);
  const db = new DatabaseSync(path); db.exec("PRAGMA foreign_keys=ON"); return { db, path };
}

test("原Rust视频三阶段真实SQLite的计费领取/候选/完成/身份绑定一致，竞争连接不重复计费", async () => {
  const { db, path } = await copiedBefore("video-hydration"), otherDb = new DatabaseSync(path), original = new DatabaseSync(fileURLToPath(new URL("after.sqlite", source)), { readOnly: true }); otherDb.exec("PRAGMA foreign_keys=ON");
  try {
    const repo = new DiscoveryVideoRepository(db), other = new DiscoveryVideoRepository(otherDb), hit = String(original.prepare("SELECT search_hit_id FROM discovery_video_completions").get()!.search_hit_id), expected = originalCandidate(original);
    assert.deepEqual(repo.next("video-hydration"), { id: hit, profile_ref: "fixture" });
    const claim = repo.claim("video-hydration", 1, "fixture", hit); assert.equal(claim.call_number, 2); assert.equal(repo.next("video-hydration"), null);
    await assert.rejects(Promise.resolve().then(() => other.claim("video-hydration", 1, "fixture", hit)), /stopped|read_active/);
    assert.equal(db.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 2);
    repo.finish(claim, expected, null);
    const candidate = db.prepare("SELECT platform,source_profile_ref,platform_account_id,display_name,profile_url,verdict,evidence_json,source_platform FROM discovery_candidates").get()!, saved = original.prepare("SELECT platform,source_profile_ref,platform_account_id,display_name,profile_url,verdict,evidence_json,source_platform FROM discovery_candidates").get()!;
    assert.deepEqual({ ...candidate, evidence_json: parseApplicationJson(String(candidate.evidence_json)) }, { ...saved, evidence_json: parseApplicationJson(String(saved.evidence_json)) });
    assert.equal(db.prepare("SELECT COUNT(*) AS count FROM discovery_video_completions").get()!.count, 1);
    assert.equal(db.prepare("SELECT COUNT(*) AS count FROM discovery_video_identity_bindings b JOIN discovery_candidates c ON c.id=b.candidate_id WHERE b.native_author_id=c.platform_account_id AND b.criteria_revision=c.criteria_revision").get()!.count, 1);
    assert.equal(repo.next("video-hydration"), null); assert.throws(() => other.finish(claim, expected, null), /claim_stale/);
  } finally { db.close(); otherDb.close(); original.close(); }
});

test("单视频超时保存真实失败收据并继续其他来源，显式读取再次计费且成功时清除该失败", async () => {
  const { db } = await copiedBefore("video-local-failure"), original = new DatabaseSync(fileURLToPath(new URL("after.sqlite", source)), { readOnly: true });
  try {
    const repo = new DiscoveryVideoRepository(db), hit = repo.next("video-hydration")!.id, first = repo.claim("video-hydration", 1, "fixture", hit);
    repo.finish(first, null, "opencli_timeout"); assert.equal(db.prepare("SELECT status FROM discovery_platform_runs").get()!.status, "completed");
    assert.equal(db.prepare("SELECT stop_code FROM discovery_video_read_failures").get()!.stop_code, "opencli_timeout"); assert.equal(repo.next("video-hydration"), null);
    const explicit = repo.claim("video-hydration", 1, "fixture", hit); assert.equal(explicit.call_number, 3); repo.finish(explicit, originalCandidate(original), null);
    assert.equal(db.prepare("SELECT COUNT(*) AS count FROM discovery_video_read_failures").get()!.count, 0);
  } finally { db.close(); original.close(); }
});

test("视频身份绑定写失败共同回滚run/candidate/completion，保留原付费领取供明确处理", async () => {
  const { db } = await copiedBefore("video-identity-rollback"), original = new DatabaseSync(fileURLToPath(new URL("after.sqlite", source)), { readOnly: true });
  try {
    const repo = new DiscoveryVideoRepository(db), hit = repo.next("video-hydration")!.id, claim = repo.claim("video-hydration", 1, "fixture", hit);
    db.exec("CREATE TRIGGER deny_video_binding BEFORE INSERT ON discovery_video_identity_bindings BEGIN SELECT RAISE(ABORT,'binding store unavailable'); END");
    assert.throws(() => repo.finish(claim, originalCandidate(original), null), /binding store unavailable/);
    assert.equal(db.prepare("SELECT COUNT(*) AS count FROM discovery_candidates").get()!.count, 0); assert.equal(db.prepare("SELECT COUNT(*) AS count FROM discovery_video_completions").get()!.count, 0);
    assert.equal(db.prepare("SELECT status FROM discovery_platform_runs").get()!.status, "searching");
    repo.interrupt(claim); assert.equal(repo.next("video-hydration")!.id, hit);
    const resumed = repo.claim("video-hydration", 1, "fixture", hit); assert.equal(resumed.call_number, 3); assert.throws(() => repo.finish(claim, originalCandidate(original), null), /claim_stale/);
    db.exec("DROP TRIGGER deny_video_binding"); repo.finish(resumed, originalCandidate(original), null);
  } finally { db.close(); original.close(); }
});
