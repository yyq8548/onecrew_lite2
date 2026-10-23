import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { copyFile, mkdir, mkdtemp, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { z } from "zod";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { discoveryWebSourceClaim, discoveryWebSourceRead, discoveryWebSourceTarget } from "../src/discovery/web-source-contracts.ts";
import { DiscoveryWebSourceRepository } from "../src/discovery/web-source-repository.ts";
import { readStorageSql } from "../src/storage/sql-resource.ts";

const fixture = (name: string) => new URL(`./fixtures/discovery-web-source/${name}`, import.meta.url);
async function open(name: string) {
  const scratch = fileURLToPath(new URL("../../.scratch/web-source-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const directory = await mkdtemp(join(scratch, `${name}-`)), path = join(directory, "creator.sqlite");
  await copyFile(fixture(`${name}-before.sqlite`), path);
  const database = new DatabaseSync(path); database.exec("PRAGMA foreign_keys=ON");
  return { database, directory, path, repository: new DiscoveryWebSourceRepository(database) };
}
async function oracle(name: string) {
  return z.strictObject({ claim: discoveryWebSourceClaim, material: z.record(z.string(), z.unknown()), result: discoveryWebSourceRead }).parse(parseApplicationJson(await readFile(fixture(`${name}.json`))));
}
function runState(db: DatabaseSync) { return db.prepare("SELECT discovery_campaign_id,platform,profile_ref,status,cursor,active_query,calls_used,stop_code FROM discovery_platform_runs ORDER BY platform").all(); }

for (const origin of ["google", "yahoo"]) for (const target of ["youtube", "twitter", "douyin"]) {
  const name = `${origin}-${target}`;
  test(`原 Rust ${name} 三阶段 SQLite：来源、计费、原文和永久作者绑定一致`, async () => {
    const input = await oracle(name), opened = await open(name);
    using claimed = new DatabaseSync(fileURLToPath(fixture(`${name}-claimed.sqlite`)), { readOnly: true });
    using finished = new DatabaseSync(fileURLToPath(fixture(`${name}-finished.sqlite`)), { readOnly: true });
    const other = new DatabaseSync(opened.path);
    try {
      const repo = opened.repository, original = input.claim;
      assert.equal(repo.next(original.campaign_id), original.search_hit_id);
      assert.equal(repo.next(original.campaign_id, [origin]), null);
      assert.equal(repo.next(original.campaign_id, [target]), original.search_hit_id);
      assert.equal(repo.next(original.campaign_id, [], [original.search_hit_id]), null);
      const claim = repo.claim(original.campaign_id, 1, original.search_hit_id);
      assert.deepEqual(claim, original);
      const actualClaimed = repo.get(original.campaign_id, original.search_hit_id);
      assert.ok(Number(actualClaimed.captured_at) >= Math.floor(Date.now() / 1000) - 5);
      assert.deepEqual({ ...actualClaimed, captured_at: 0 }, { ...new DiscoveryWebSourceRepository(claimed).get(original.campaign_id, original.search_hit_id), captured_at: 0 });
      assert.deepEqual(runState(opened.database), runState(claimed));
      assert.throws(() => new DiscoveryWebSourceRepository(other).claim(original.campaign_id, 1, original.search_hit_id), /attempted_or_stopped/);
      const result = repo.finish(claim, input.material, null);
      assert.deepEqual({ ...result, captured_at: 0, candidate_id: null }, { ...input.result, captured_at: 0, candidate_id: null });
      assert.ok(result.candidate_id);
      const actual = opened.database.prepare("SELECT * FROM discovery_candidates").get()!, expected = finished.prepare("SELECT * FROM discovery_candidates").get()!;
      assert.equal(actual.id, result.candidate_id);
      const actualEvidence = z.record(z.string(), z.unknown()).parse(parseApplicationJson(Buffer.from(String(actual.evidence_json))));
      const expectedEvidence = z.record(z.string(), z.unknown()).parse(parseApplicationJson(Buffer.from(String(expected.evidence_json))));
      if (target === "twitter") {
        const expectedAssessment = z.record(z.string(), z.unknown()).parse(expectedEvidence.assessment);
        expectedAssessment.subscribers_normalized = z.record(z.string(), z.unknown()).parse(input.material.profile).follower_count;
        expectedEvidence.assessment = expectedAssessment;
      }
      assert.deepEqual({ ...actualEvidence, captured_at: 0 }, { ...expectedEvidence, captured_at: 0 });
      for (const key of ["discovery_campaign_id", "criteria_revision", "platform", "source_profile_ref", "platform_account_id", "display_name", "profile_url", "verdict", "source_platform"]) assert.equal(actual[key], expected[key]);
      const binding = opened.database.prepare("SELECT * FROM discovery_video_identity_bindings").get()!;
      assert.equal(binding.candidate_id, actual.id); assert.equal(binding.native_author_id, actual.platform_account_id);
      assert.equal(binding.search_hit_id, original.search_hit_id);
      assert.equal(opened.database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, original.call_number);
      assert.deepEqual(runState(opened.database), runState(finished));
      assert.throws(() => repo.claim(original.campaign_id, 1, original.search_hit_id));
      assert.throws(() => repo.finish(claim, input.material, null), /claim_stale/);
    } finally { other.close(); opened.database.close(); await rm(opened.directory, { recursive: true }); }
  });
}

test("历史读取上限移除后继续保留批准、未知领取和来源停止校验", async () => {
  const input = await oracle("google-twitter"), opened = await open("google-twitter");
  try {
    const repo = opened.repository, claim = input.claim;
    opened.database.exec("UPDATE discovery_campaigns SET max_calls=3");
    opened.database.exec(readStorageSql("creator-93-to-94.sql"));
    assert.equal(opened.database.prepare("SELECT max_calls FROM discovery_campaigns").get()!.max_calls, null);
    assert.equal(repo.next(claim.campaign_id), claim.search_hit_id);
    assert.equal(opened.database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 1);
    assert.throws(() => repo.claim(claim.campaign_id, 2, claim.search_hit_id));
    const issued = repo.claim(claim.campaign_id, 1, claim.search_hit_id);
    assert.equal(repo.next(claim.campaign_id), null);
    assert.throws(() => repo.claim(claim.campaign_id, 1, claim.search_hit_id));
    assert.throws(() => repo.finish({ ...issued, call_number: issued.call_number + 1 }, input.material, null), /claim_stale/);
    assert.equal(repo.get(claim.campaign_id, claim.search_hit_id).status, "reading");
    repo.finish(issued, null, "opencli_risk_control");
    assert.equal(repo.get(claim.campaign_id, claim.search_hit_id).status, "failed");
    assert.equal(opened.database.prepare("SELECT status,stop_code FROM discovery_platform_runs WHERE platform='google'").get()!.status, "completed");
    assert.equal(opened.database.prepare("SELECT COUNT(*) total FROM discovery_candidates").get()!.total, 0);
    assert.equal(opened.database.prepare("SELECT COUNT(*) total FROM discovery_platform_runs WHERE platform='twitter'").get()!.total, 0);
    assert.equal(opened.database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 4);
  } finally { opened.database.close(); await rm(opened.directory, { recursive: true }); }
});

test("原文无效或作者绑定事务失败时保留领取并回滚整次写入", async () => {
  const input = await oracle("yahoo-douyin"), opened = await open("yahoo-douyin");
  try {
    const repo = opened.repository, original = input.claim, claim = repo.claim(original.campaign_id, 1, original.search_hit_id);
    const invalid = structuredClone(input.material); invalid.content = { ...z.record(z.string(), z.unknown()).parse(invalid.content), text_truncated: true };
    assert.throws(() => repo.finish(claim, invalid, null));
    assert.equal(repo.get(original.campaign_id, original.search_hit_id).status, "reading");
    opened.database.exec("CREATE TRIGGER reject_web_binding BEFORE INSERT ON discovery_video_identity_bindings BEGIN SELECT RAISE(ABORT,'binding rejected'); END");
    assert.throws(() => repo.finish(claim, input.material, null), /binding rejected/);
    assert.equal(repo.get(original.campaign_id, original.search_hit_id).status, "reading");
    assert.equal(opened.database.prepare("SELECT COUNT(*) total FROM discovery_candidates").get()!.total, 0);
    assert.equal(opened.database.prepare("SELECT COUNT(*) total FROM discovery_video_completions").get()!.total, 0);
    assert.equal(opened.database.prepare("SELECT status FROM discovery_platform_runs").get()!.status, "searching");
  } finally { opened.database.close(); await rm(opened.directory, { recursive: true }); }
});

test("原生目标识别保留严格 HTTPS、重复参数及大小写规则", () => {
  assert.equal(discoveryWebSourceTarget("https://www.youtube.com/watch?v=abcdefghijk&v=abcdefghijk"), null);
  assert.equal(discoveryWebSourceTarget("https://www.youtube.com/watch?v=abcdefghijk#bad"), null);
  assert.equal(discoveryWebSourceTarget("http://www.youtube.com/watch?v=abcdefghijk"), null);
  assert.equal(discoveryWebSourceTarget("https://user@x.com/maker/status/123"), null);
  assert.deepEqual(discoveryWebSourceTarget("https://x.com/Maker/status/123"), { platform: "twitter", id: "123", cost: 3, handle: "maker" });
  assert.deepEqual(discoveryWebSourceTarget("https://www.youtube.com/watch/abcdefghijk"), { platform: "youtube", id: "abcdefghijk", cost: 1 });
});

test("六组原生来源冻结 oracle 哈希完整，默认执行全部十八数据库", async () => {
  const hashes = z.record(z.string(), z.string().regex(/^[a-f0-9]{64}$/u)).parse(parseApplicationJson(await readFile(fixture("sha256.json"))));
  assert.equal(Object.keys(hashes).length, 24);
  for (const [name, hash] of Object.entries(hashes)) assert.equal(createHash("sha256").update(await readFile(fixture(name))).digest("hex"), hash);
});
