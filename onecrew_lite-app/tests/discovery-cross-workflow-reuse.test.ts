import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { z } from "zod";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { DiscoverySourceAuthorRepository } from "../src/discovery/source-author-repository.ts";
import { DiscoveryWebSourceRepository } from "../src/discovery/web-source-repository.ts";

test("抖音联合读取及网页作者核实复用其他任务的真实成功主页，并隔离浏览器身份", () => {
  using db = new DatabaseSync(fileURLToPath(new URL("./fixtures/discovery-source-author/completed.sqlite", import.meta.url)), { readOnly: true });
  const row = db.prepare("SELECT search_hit_id,discovery_campaign_id,author_id FROM discovery_douyin_author_reads WHERE status='completed' LIMIT 1").get()!;
  const repository = new DiscoverySourceAuthorRepository(db);
  const receipt = repository.get(String(row.discovery_campaign_id), String(row.search_hit_id));
  const profile = z.record(z.string(), z.unknown()).parse(parseApplicationJson(String(db.prepare("SELECT native_profile_json FROM discovery_profile_reads WHERE search_hit_id=?").get(String(row.search_hit_id))!.native_profile_json)));
  const next = { ...receipt.claim, campaign_id: receipt.claim.campaign_id + "-next", expected_revision: receipt.claim.expected_revision + 1 };
  assert.deepEqual(repository.savedProfile(next, String(row.author_id)), profile);
  assert.equal(repository.savedProfile({ ...next, profile_ref: next.profile_ref + "-other" }, String(row.author_id)), null);
  const web = new DiscoveryWebSourceRepository(db);
  assert.deepEqual(web.savedProfile(next.campaign_id, next.expected_revision, next.profile_ref, "douyin", String(profile.profile_url)), profile);
  assert.equal(web.savedProfile(next.campaign_id, next.expected_revision, next.profile_ref + "-other", "douyin", String(profile.profile_url)), null);
});

test("网页来源复用其他任务保存的原生主页，不跨原平台或浏览器身份借用", () => {
  using db = new DatabaseSync(fileURLToPath(new URL("./fixtures/discovery-web-source/google-douyin-finished.sqlite", import.meta.url)), { readOnly: true });
  const oracle = z.object({ claim: z.object({ campaign_id: z.string(), expected_revision: z.number(), profile_ref: z.string() }), material: z.object({ profile: z.record(z.string(), z.unknown()) }) }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/discovery-web-source/google-douyin.json", import.meta.url))));
  const profile = oracle.material.profile, claim = oracle.claim, repository = new DiscoveryWebSourceRepository(db);
  const campaign = claim.campaign_id + "-next";
  assert.deepEqual(repository.savedProfile(campaign, claim.expected_revision + 1, claim.profile_ref, "douyin", String(profile.profile_url)), profile);
  assert.equal(repository.savedProfile(campaign, claim.expected_revision + 1, claim.profile_ref + "-other", "douyin", String(profile.profile_url)), null);
  assert.equal(repository.savedProfile(campaign, claim.expected_revision + 1, claim.profile_ref, "twitter", String(profile.profile_url)), null);
});
