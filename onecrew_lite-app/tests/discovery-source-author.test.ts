import assert from "node:assert/strict";
import { copyFile, mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { DiscoverySourceAuthorRepository } from "../src/discovery/source-author-repository.ts";

const campaign = "douyin-source-flow";
async function before() {
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/source-author-", import.meta.url))), path = join(root, "crm.sqlite"); await copyFile(new URL("./fixtures/discovery-source-author/before.sqlite", import.meta.url), path); const db = new DatabaseSync(path); db.exec("PRAGMA foreign_keys=ON"); return { db, path };
}
function material(original: DatabaseSync) {
  const profile = original.prepare("SELECT native_profile_json FROM discovery_profile_reads").get()!, content = original.prepare("SELECT native_content_json FROM discovery_content_reads").get()!;
  return { profile: parseApplicationJson(String(profile.native_profile_json)), content: parseApplicationJson(String(content.native_content_json)) as Record<string, unknown> };
}
test("原Rust Douyin联合观察事务保存作者/profile/content/候选，重复作者只记duplicate不投影第二份", async () => {
  const { db, path } = await before(); using other = new DatabaseSync(path); using completed = new DatabaseSync(fileURLToPath(new URL("./fixtures/discovery-source-author/completed.sqlite", import.meta.url)), { readOnly: true }); using duplicate = new DatabaseSync(fileURLToPath(new URL("./fixtures/discovery-source-author/duplicate.sqlite", import.meta.url)), { readOnly: true });
  try {
    const repo = new DiscoverySourceAuthorRepository(db), hit = repo.next(campaign)!, expected = new DiscoverySourceAuthorRepository(completed).get(campaign, hit), native = material(completed), claim = repo.claim(campaign, 1, hit); assert.deepEqual(claim, expected.claim);
    assert.throws(() => new DiscoverySourceAuthorRepository(other).claim(campaign, 1, hit), /already_attempted_or_stopped/);
    const receipt = repo.finish(claim, native.profile, native.content, null); assert.deepEqual({ ...receipt, captured_at: 0 }, { ...expected, captured_at: 0 });
    const selected = "platform,source_profile_ref,platform_account_id,display_name,profile_url,verdict,evidence_json,source_platform", candidate = { ...db.prepare(`SELECT ${selected} FROM discovery_candidates`).get()! }, original = { ...completed.prepare(`SELECT ${selected} FROM discovery_candidates`).get()! };
    const evidence = parseApplicationJson(String(candidate.evidence_json)) as Record<string, unknown>, originalEvidence = parseApplicationJson(String(original.evidence_json)) as Record<string, unknown>; delete evidence.captured_at; delete originalEvidence.captured_at; assert.deepEqual({ ...candidate, evidence_json: evidence }, { ...original, evidence_json: { ...originalEvidence, profile_bio_status: "complete", profile_bio_status_source_url: candidate.profile_url } }); assert.equal(evidence.subscribers_raw, "不可见");
    const secondHit = repo.next(campaign)!, second = repo.claim(campaign, 1, secondHit), originalDuplicate = new DiscoverySourceAuthorRepository(duplicate).get(campaign, secondHit);
    const nextContent = { ...native.content, post_id: second.post_id, source_url: second.source_url };
    assert.deepEqual({ ...repo.finish(second, native.profile, nextContent, null), captured_at: 0 }, { ...originalDuplicate, captured_at: 0 });
    assert.equal(db.prepare("SELECT COUNT(*) n FROM discovery_profile_reads").get()!.n, 1); assert.equal(db.prepare("SELECT COUNT(*) n FROM discovery_candidates").get()!.n, 1); assert.equal(db.prepare("SELECT COUNT(*) n FROM discovery_search_authors").get()!.n, 2); assert.equal(repo.next(campaign), null);
  } finally { db.close(); }
});
test("联合观察写失败共同回滚所有身份材料，取消永久结算原来源后继续另一个匹配视频", async () => {
  const { db } = await before(); using original = new DatabaseSync(fileURLToPath(new URL("./fixtures/discovery-source-author/completed.sqlite", import.meta.url)), { readOnly: true });
  try {
    const repo = new DiscoverySourceAuthorRepository(db), hit = repo.next(campaign)!, claim = repo.claim(campaign, 1, hit), native = material(original);
    db.exec("CREATE TRIGGER reject_joint_candidate BEFORE INSERT ON discovery_candidates BEGIN SELECT RAISE(ABORT,'joint_candidate_rejected'); END");
    assert.throws(() => repo.finish(claim, native.profile, native.content, null), /joint_candidate_rejected/); assert.equal(db.prepare("SELECT COUNT(*) n FROM discovery_search_authors").get()!.n, 0); assert.equal(db.prepare("SELECT COUNT(*) n FROM discovery_profile_reads").get()!.n, 0); assert.equal(repo.get(campaign, hit).status, "reading");
    db.exec("DROP TRIGGER reject_joint_candidate"); repo.interrupt(claim); assert.equal(repo.get(campaign, hit).stop_code, "interrupted_source_author_read"); assert.notEqual(repo.next(campaign), hit); assert.throws(() => repo.claim(campaign, 1, hit), /already_attempted_or_stopped/);
  } finally { db.close(); }
});
