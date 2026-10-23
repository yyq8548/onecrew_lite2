import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { copyFile, mkdir, mkdtemp, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { z } from "zod";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { discoveryLanguageClaim, validDiscoveryLanguageSample } from "../src/discovery/language-contracts.ts";
import { DiscoveryLanguageRepository } from "../src/discovery/language-repository.ts";
import { discoveryLanguageSample } from "../src/discovery/review-material.ts";

const fixture = (name: string) => new URL(`./fixtures/discovery-language/${name}`, import.meta.url);
const input = z.strictObject({ claim: discoveryLanguageClaim, sample: discoveryLanguageSample }).parse(parseApplicationJson(await readFile(fixture("tiktok.json"))));
async function open() {
  const scratch = fileURLToPath(new URL("../../.scratch/language-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const directory = await mkdtemp(join(scratch, "language-")), path = join(directory, "creator.sqlite");
  await copyFile(fixture("tiktok-before.sqlite"), path);
  const database = new DatabaseSync(path); database.exec("PRAGMA foreign_keys=ON");
  return { directory, database, path, repository: new DiscoveryLanguageRepository(database) };
}
function comparable(database: DatabaseSync, table: "discovery_campaigns" | "discovery_platform_runs" | "discovery_language_samples") {
  return database.prepare(`SELECT * FROM ${table}`).all().map(row => Object.fromEntries(Object.entries(row).filter(([key]) => key !== "created_at" && key !== "updated_at")));
}

test("原 Rust 语言证据三阶段 SQLite：计费、并发领取及完成逐字段一致", async () => {
  const opened = await open();
  using claimed = new DatabaseSync(fileURLToPath(fixture("tiktok-claimed.sqlite")), { readOnly: true });
  using finished = new DatabaseSync(fileURLToPath(fixture("tiktok-finished.sqlite")), { readOnly: true });
  const other = new DatabaseSync(opened.path);
  try {
    const repo = opened.repository, original = input.claim;
    assert.equal(repo.next(original.campaign_id, original.platform), original.search_hit_id);
    assert.equal(repo.next(original.campaign_id, original.platform, [original.search_hit_id]), null);
    assert.equal(repo.next(original.campaign_id, "unsupported"), null);
    const claim = repo.claim(original.campaign_id, original.revision, original.search_hit_id);
    assert.deepEqual(claim, original);
    assert.equal(new DiscoveryLanguageRepository(other).claim(original.campaign_id, original.revision, original.search_hit_id), null);
    for (const table of ["discovery_campaigns", "discovery_platform_runs", "discovery_language_samples"] as const) assert.deepEqual(comparable(opened.database, table), comparable(claimed, table));
    assert.deepEqual(repo.source(original.campaign_id, original.search_hit_id), { platform: "tiktok", profile_ref: "dedicated" });
    assert.deepEqual(repo.readContext(claim!), { body: null, profile: null, web: null, joint: null });
    assert.throws(() => repo.finish(claim!, { ...input.sample, author_id: "999" }, null), /language_sample_invalid/);
    repo.finish(claim!, input.sample, null);
    for (const table of ["discovery_campaigns", "discovery_platform_runs", "discovery_language_samples"] as const) assert.deepEqual(comparable(opened.database, table), comparable(finished, table));
    assert.equal(repo.claim(original.campaign_id, original.revision, original.search_hit_id), null);
    assert.throws(() => repo.finish(claim!, input.sample, null), /language_claim_changed/);
    assert.equal(opened.database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, original.call_number);
  } finally { other.close(); opened.database.close(); await rm(opened.directory, { recursive: true }); }
});

test("语言原文或审核变化不扣费，完成失败回滚，未知领取不重复", async () => {
  const opened = await open();
  try {
    const repo = opened.repository, original = input.claim;
    opened.database.prepare("UPDATE discovery_search_hits SET title='Changed original caption' WHERE id=?").run(original.search_hit_id);
    assert.equal(repo.claim(original.campaign_id, 1, original.search_hit_id), null);
    assert.equal(opened.database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 2);
    opened.database.prepare("UPDATE discovery_search_hits SET title='Clothing fabrics' WHERE id=?").run(original.search_hit_id);
    assert.throws(() => repo.claim(original.campaign_id, 2, original.search_hit_id));
    const claim = repo.claim(original.campaign_id, 1, original.search_hit_id)!;
    assert.equal(repo.claim(original.campaign_id, 1, original.search_hit_id), null);
    assert.throws(() => repo.finish({ ...claim, call_number: claim.call_number + 1 }, input.sample, null), /language_claim_changed/);
    assert.equal(opened.database.prepare("SELECT status FROM discovery_language_samples").get()!.status, "reading");
    repo.finish(claim, null, "opencli_timeout");
    assert.equal(repo.next(original.campaign_id, "tiktok"), null);
    assert.equal(opened.database.prepare("SELECT status,stop_code FROM discovery_platform_runs").get()!.status, "completed");
    assert.equal(opened.database.prepare("SELECT status FROM discovery_language_samples").get()!.status, "failed");
  } finally { opened.database.close(); await rm(opened.directory, { recursive: true }); }
});

test("原语言样本只使用匹配原文首五条，拒绝额外来源、评论者身份及重复评论", () => {
  assert.equal(validDiscoveryLanguageSample(input.sample, input.claim), true);
  assert.equal(validDiscoveryLanguageSample({ ...input.sample, comments: Array.from({ length: 5 }, (_, index) => ({ source_url: input.claim.root_url, text: `Substantive comment ${index + 1}` })) }, input.claim), true);
  assert.equal(validDiscoveryLanguageSample({ ...input.sample, comments: Array.from({ length: 6 }, (_, index) => ({ source_url: input.claim.root_url, text: `Substantive comment ${index + 1}` })) }, input.claim), false);
  assert.equal(validDiscoveryLanguageSample(input.sample, { ...input.claim, ordinal: 2 }), false);
  assert.equal(validDiscoveryLanguageSample(input.sample, { ...input.claim, previous_urls: [input.claim.root_url] }), false);
  assert.equal(validDiscoveryLanguageSample({ ...input.sample, comments: [{ ...input.sample.comments[0], author_id: "unneeded" }] }, input.claim), false);
  assert.equal(validDiscoveryLanguageSample({ ...input.sample, comments: [input.sample.comments[0], input.sample.comments[0]] }, input.claim), false);
});

test("冻结原始语言 oracle 文件完整且默认执行", async () => {
  for (const [name, hash] of [
    ["tiktok-before.sqlite", "be34e800e5642b98b4bf4ab14e7ebb07d7951173483c98b8588e794305bf8dcd"],
    ["tiktok-claimed.sqlite", "dc1a1b8a460537a5c25a17ad233975c69027a7000f18661ebedfe92b54f81045"],
    ["tiktok-finished.sqlite", "7951fa9c970aeafcd3e72411eca33b6f2faa9727e346ec5d6b0ed9bf19b65ff6"],
    ["tiktok.json", "c0857d2027d0412944ec627c81d3878d99cad054daece695d77275b0d04213b5"],
  ]) assert.equal(createHash("sha256").update(await readFile(fixture(name!))).digest("hex"), hash);
});
