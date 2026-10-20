import assert from "node:assert/strict";
import { copyFile, mkdtemp, readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { DiscoveryProfileRepository } from "../src/discovery/profile-repository.ts";

test("原Rust真实三阶段SQLite的profile付费领取/回执/候选证据全部一致，双连接不重复付费", async () => {
  const source = new URL("./fixtures/discovery-profile-hydration/", import.meta.url), root = await mkdtemp(fileURLToPath(new URL("../../.scratch/profile-hydration-", import.meta.url))), path = join(root, "crm.sqlite");
  await copyFile(new URL("before.sqlite", source), path);
  const before = await readFile(new URL("before.sqlite", source)), db = new DatabaseSync(path), competing = new DatabaseSync(path), originalClaimed = new DatabaseSync(fileURLToPath(new URL("claimed.sqlite", source)), { readOnly: true }), originalAfter = new DatabaseSync(fileURLToPath(new URL("after.sqlite", source)), { readOnly: true });
  try {
    const original = new DiscoveryProfileRepository(originalAfter), hit = String(originalAfter.prepare("SELECT search_hit_id FROM discovery_profile_reads").get()!.search_hit_id), expected = original.get("profile-hydration", hit);
    const repo = new DiscoveryProfileRepository(db), other = new DiscoveryProfileRepository(competing);
    assert.equal(repo.next("profile-hydration"), hit); assert.equal(repo.next("profile-hydration", ["pinterest"]), null); assert.equal(repo.next("profile-hydration", [], [hit]), null);
    const claim = repo.claim("profile-hydration", 1, hit); assert.equal(repo.next("profile-hydration"), null);
    assert.deepEqual(claim, new DiscoveryProfileRepository(originalClaimed).get("profile-hydration", hit).claim);
    assert.equal(db.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 2);
    assert.throws(() => other.claim("profile-hydration", 1, hit), /read_active/);
    assert.equal(competing.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 2);
    assert.throws(() => repo.finish(claim, { ...expected.native_profile, author_id: "other" }, null));
    assert.equal(repo.get("profile-hydration", hit).status, "reading");
    const receipt = repo.finish(claim, expected.native_profile, null);
    assert.deepEqual({ ...receipt, captured_at: expected.captured_at }, expected);
    assert.deepEqual(other.finish(claim, expected.native_profile, null), receipt);
    assert.equal(repo.next("profile-hydration"), null);
    assert.throws(() => other.claim("profile-hydration", 1, hit), /already_attempted/);
    const candidate = db.prepare("SELECT platform,source_profile_ref,platform_account_id,display_name,profile_url,verdict,evidence_json,source_platform FROM discovery_candidates").get()!;
    const originalCandidate = originalAfter.prepare("SELECT platform,source_profile_ref,platform_account_id,display_name,profile_url,verdict,evidence_json,source_platform FROM discovery_candidates").get()!;
    const evidence = parseApplicationJson(String(candidate.evidence_json)) as Record<string, unknown>, originalEvidence = parseApplicationJson(String(originalCandidate.evidence_json)) as Record<string, unknown>;
    assert.equal(evidence.captured_at, receipt.captured_at);
    assert.deepEqual({ ...candidate, evidence_json: { ...evidence, captured_at: originalEvidence.captured_at } }, { ...originalCandidate, evidence_json: { ...originalEvidence, pinterest_monthly_views_raw: null, pinterest_monthly_views_source_url: candidate.profile_url, profile_bio_status: "unknown", profile_bio_status_source_url: candidate.profile_url } });
    assert.equal(db.prepare("SELECT status FROM discovery_platform_runs").get()!.status, "completed");
  } finally { db.close(); competing.close(); originalClaimed.close(); originalAfter.close(); }
  assert.equal(createHash("sha256").update(await readFile(new URL("before.sqlite", source))).digest("hex"), createHash("sha256").update(before).digest("hex"));
});

test("profile完成回执与候选写入同一真实事务，候选写失败不丢失领取且不制造成功", async () => {
  const source = new URL("./fixtures/discovery-profile-hydration/", import.meta.url), root = await mkdtemp(fileURLToPath(new URL("../../.scratch/profile-hydration-rollback-", import.meta.url))), path = join(root, "crm.sqlite");
  await copyFile(new URL("claimed.sqlite", source), path);
  const db = new DatabaseSync(path), original = new DatabaseSync(fileURLToPath(new URL("after.sqlite", source)), { readOnly: true });
  try {
    const hit = String(original.prepare("SELECT search_hit_id FROM discovery_profile_reads").get()!.search_hit_id), expected = new DiscoveryProfileRepository(original).get("profile-hydration", hit), repo = new DiscoveryProfileRepository(db);
    db.exec("CREATE TRIGGER reject_candidate BEFORE INSERT ON discovery_candidates BEGIN SELECT RAISE(ABORT,'candidate store unavailable'); END");
    assert.throws(() => repo.finish(expected.claim, expected.native_profile, null), /candidate store unavailable/);
    assert.equal(repo.get("profile-hydration", hit).status, "reading");
    assert.equal(db.prepare("SELECT status FROM discovery_platform_runs").get()!.status, "searching");
    assert.equal(db.prepare("SELECT COUNT(*) AS count FROM discovery_candidates").get()!.count, 0);
    db.exec("DROP TRIGGER reject_candidate");
    assert.equal(repo.finish(expected.claim, expected.native_profile, null).status, "completed");
    db.prepare("UPDATE discovery_profile_reads SET updated_at=?").run(9223372036854775807n);
    assert.equal(repo.get("profile-hydration", hit).captured_at, 9223372036854775807n);
  } finally { db.close(); original.close(); }
});

test("原中断profile读取保留已计费领取，新读取增加计费且旧回执不可覆盖新领取", async () => {
  const source = new URL("./fixtures/discovery-profile-hydration/", import.meta.url), root = await mkdtemp(fileURLToPath(new URL("../../.scratch/profile-hydration-interrupted-", import.meta.url))), path = join(root, "crm.sqlite");
  await copyFile(new URL("claimed.sqlite", source), path);
  const db = new DatabaseSync(path), original = new DatabaseSync(fileURLToPath(new URL("after.sqlite", source)), { readOnly: true });
  try {
    const hit = String(original.prepare("SELECT search_hit_id FROM discovery_profile_reads").get()!.search_hit_id), expected = new DiscoveryProfileRepository(original).get("profile-hydration", hit), repo = new DiscoveryProfileRepository(db);
    assert.equal(repo.recoverInterrupted(), 1); assert.equal(repo.recoverInterrupted(), 0);
    assert.equal(repo.next("profile-hydration"), hit);
    assert.equal(repo.get("profile-hydration", hit).status, "reading");
    assert.equal(db.prepare("SELECT stop_code FROM discovery_platform_runs").get()!.stop_code, "interrupted_profile_read");
    const resumed = repo.claim("profile-hydration", 1, hit); assert.equal(resumed.call_number, 3);
    assert.equal(db.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 3);
    assert.throws(() => repo.finish(expected.claim, expected.native_profile, null), /claim_stale/);
    assert.equal(repo.finish(resumed, expected.native_profile, null).status, "completed");
  } finally { db.close(); original.close(); }
});
