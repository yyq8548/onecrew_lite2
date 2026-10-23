import assert from "node:assert/strict";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { loadDiscoveryReviewBasis } from "../src/discovery/review-basis.ts";
import { buildDiscoveryReviewMaterial, containsDiscoveryCitation } from "../src/discovery/review-material.ts";
import { discoveryObservationVersion, pendingReview } from "../src/discovery/pending-review.ts";
import { discoveryMatchPromptMaterial } from "../src/discovery/semantic-match.ts";

test("真实TikTok新增简介完整性依据使旧评分待更新，回执哈希不改变观察版本", () => {
  using db = new DatabaseSync(fileURLToPath(new URL("./fixtures/discovery-language/tiktok-before.sqlite", import.meta.url)), { readOnly: true });
  const row = db.prepare("SELECT c.id,c.discovery_campaign_id AS campaign,c.criteria_revision AS revision,h.id AS hit FROM discovery_candidates c JOIN discovery_search_hits h ON h.discovery_campaign_id=c.discovery_campaign_id AND h.source_url=json_extract(c.evidence_json,'$.source_urls[0]') WHERE c.verdict='pending' LIMIT 1").get()!;
  db.exec("BEGIN DEFERRED");
  const basis = loadDiscoveryReviewBasis(db, String(row.campaign), Number(row.revision), String(row.hit)), material = buildDiscoveryReviewMaterial(basis);
  const prompt = discoveryMatchPromptMaterial(material, basis.native_followers);
  assert.deepEqual(prompt.native_profile, material.native_profile);
  const review = db.prepare("SELECT material_json FROM discovery_evidence_reviews WHERE candidate_id=? ORDER BY created_at DESC LIMIT 1").get(String(row.id))!;
  const saved = parseApplicationJson(Buffer.from(String(review.material_json)));
  assert.notEqual(discoveryObservationVersion(saved), discoveryObservationVersion(material));
  const { material_hash, candidate_evidence_hash, ...observations } = material;
  assert.equal(discoveryObservationVersion(observations), discoveryObservationVersion(material));
  assert.equal(material.native_profile?.bio_status, "complete");
  const state = pendingReview(db, String(row.campaign), Number(row.revision), String(row.id));
  assert.equal(state?.state, "awaiting_score");
  assert.ok(state.last_processed_at > 0);
  db.exec("ROLLBACK");
});

test("真实X材料未保存简介时不把账号名称当作简介引用", () => {
  using db = new DatabaseSync(fileURLToPath(new URL("./fixtures/discovery-content-hydration/twitter-after.sqlite", import.meta.url)), { readOnly: true });
  const row = db.prepare("SELECT discovery_campaign_id AS campaign,expected_revision AS revision,search_hit_id AS hit FROM discovery_content_reads WHERE status='completed' LIMIT 1").get()!;
  db.exec("BEGIN DEFERRED");
  const basis = loadDiscoveryReviewBasis(db, String(row.campaign), Number(row.revision), String(row.hit)), material = buildDiscoveryReviewMaterial(basis);
  assert.equal(material.native_profile?.bio ?? null, null);
  assert.ok(material.native_profile?.profile_url);
  assert.equal(containsDiscoveryCitation(material, { kind: "native_profile", profile_url: material.native_profile.profile_url, quote: material.hit.author_label }), false);
  assert.equal(containsDiscoveryCitation(material, { kind: "native_profile", profile_url: "https://x.com/unrelated", quote: material.hit.author_label }), false);
  db.exec("ROLLBACK");
});
