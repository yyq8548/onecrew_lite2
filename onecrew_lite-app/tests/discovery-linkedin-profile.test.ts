import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test, type TestContext } from "node:test";
import { fileURLToPath } from "node:url";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../src/contracts/application-json.ts";
import { initializeNewCreatorDatabase } from "../src/storage/creator-schema.ts";
import { DiscoveryRepository } from "../src/discovery/repository.ts";
import { DiscoveryContentRepository } from "../src/discovery/content-repository.ts";
import { pendingReview } from "../src/discovery/pending-review.ts";
import { containsDiscoveryCitation, discoveryReviewBasis } from "../src/discovery/review-material.ts";
import { prepareDiscoveryReview } from "../src/discovery/review-submission.ts";
import { buildDiscoveryMatchPrompt, discoveryMatchEvidenceKey, discoveryMatchPromptMaterial, loadBundledDiscoveryMatchRubric } from "../src/discovery/semantic-match.ts";
import { linkedinConnections, linkedinProfilePlan, linkedinContactPlan } from "../src/platforms/opencli/linkedin.ts";
import { DiscoveryPublicPageRepository, validateDiscoveryPublicPageResult } from "../src/discovery/public-page-repository.ts";

const row = z.record(z.string(), z.union([z.string(), z.number(), z.null()]));
const captured = z.object({ plan: row, hit: row, author: row, profile: row, content: row, candidate: row }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-profile-or-post-20261020.json", import.meta.url))));
const campaign = z.string().parse(captured.plan.discovery_campaign_id), hit = z.string().parse(captured.hit.id), candidate = z.string().parse(captured.candidate.id);

async function replay(t: TestContext, failedContent: boolean, observations = captured) {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/linkedin-profile-", import.meta.url))), path = join(directory, "creator.sqlite");
  await initializeNewCreatorDatabase(path);
  const db = new DatabaseSync(path); t.after(() => db.close());
  const repository = new DiscoveryRepository(db);
  repository.create({ id: campaign, brief: parseApplicationJson(Buffer.from(String(captured.plan.brief_json))), opencli_version: "1.8.8", criteria: parseApplicationJson(Buffer.from(String(captured.plan.criteria_json))), scopes: parseApplicationJson(Buffer.from(String(captured.plan.scope_json))), target_count: 30, max_calls: null });
  repository.approve(campaign, 1);
  db.prepare("UPDATE discovery_campaign_revisions SET approved_at=? WHERE discovery_campaign_id=?").run(captured.plan.approved_at!, campaign);
  db.prepare("UPDATE discovery_platform_runs SET status='completed' WHERE discovery_campaign_id=?").run(campaign);
  const records = { discovery_search_hits: observations.hit, discovery_search_authors: observations.author, discovery_profile_reads: observations.profile, discovery_candidates: observations.candidate, ...(failedContent ? { discovery_content_reads: observations.content } : {}) };
  for (const [table, value] of Object.entries(records)) {
    const columns = Object.keys(value);
    db.prepare(`INSERT INTO ${table}(${columns.join(",")}) VALUES(${columns.map(() => "?").join(",")})`).run(...Object.values(value));
  }
  return { db, repository };
}

test("真实Ray主页职位可独立审核，空简介和没有近期帖子不阻断评分", async t => {
  const { db, repository } = await replay(t, true);
  const material = repository.prepareReviewMaterial(campaign, 1, hit);
  assert.equal(material.native_profile?.web_text, "Global Business Development at Binance | Payments");
  assert.equal(material.native_content ?? null, null);
  assert.equal(material.native_author_id, captured.candidate.platform_account_id);
  const citation = { kind: "native_profile" as const, profile_url: String(captured.hit.source_url), quote: material.native_profile!.web_text! };
  assert.equal(containsDiscoveryCitation(material, citation), true);
  assert.equal(containsDiscoveryCitation(material, { ...citation, profile_url: String(captured.hit.source_url) + "overlay/contact-info/" }), false);
  db.exec("BEGIN DEFERRED");
  const pending = pendingReview(db, campaign, 1, candidate);
  assert.equal(pending?.state, "awaiting_score");
  assert.equal(pending.stop_code, null);
  db.exec("COMMIT");
  assert.equal(db.prepare("SELECT stop_code FROM discovery_content_reads WHERE search_hit_id=?").get(hit)?.stop_code, "linkedin_content_unavailable");
  assert.equal(new DiscoveryContentRepository(db).nextNative(campaign, "linkedin"), null);
  db.prepare("DELETE FROM discovery_search_authors WHERE search_hit_id=?").run(hit);
  assert.throws(() => repository.prepareReviewMaterial(campaign, 1, hit), /author_binding_unavailable/);
});

test("真实Ray主页完成后先评分，不自动读取帖子，不消耗读取预算", async t => {
  const { db, repository } = await replay(t, false);
  const before = repository.getPlan(campaign).used_calls;
  assert.equal(new DiscoveryContentRepository(db).nextNative(campaign, "linkedin"), null);
  assert.equal(repository.getPlan(campaign).used_calls, before);
  assert.ok(repository.prepareInitialMatchMaterial(campaign, 1, hit).material.native_profile?.web_text);
});

test("LinkedIn新规则明确任一来源和当前任职，旧评分缓存不能代替新规则评分", async t => {
  const { repository } = await replay(t, false), material = repository.prepareReviewMaterial(campaign, 1, hit);
  const rubric = await loadBundledDiscoveryMatchRubric(), prompt = buildDiscoveryMatchPrompt("搜索交易所商务人员", material, discoveryMatchPromptMaterial(material, 1548), rubric);
  assert.match(prompt, /LinkedIn evidence sources are alternatives/);
  assert.match(prompt, /former|past employment/);
  const legacy = `initial-v2-${createHash("sha256").update(`${rubric}:${stringifyApplicationJson(discoveryMatchPromptMaterial(material, 1548))}:automatic`).digest("hex")}`;
  assert.notEqual(discoveryMatchEvidenceKey(material, 1548, rubric, false), legacy);
});

test("真实LinkedIn帖子继续参与审核，已经推荐的原生账号不再读帖，跨账号帖子明确拒绝", async t => {
  const { db, repository } = await replay(t, false);
  const post = z.object({ hit: row, author: row, profile: row, content: row, candidate: row }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-post-control-20261020.json", import.meta.url))));
  for (const [table, value] of Object.entries({ discovery_search_hits: post.hit, discovery_search_authors: post.author, discovery_profile_reads: post.profile, discovery_candidates: post.candidate, discovery_content_reads: post.content })) {
    const columns = Object.keys(value);
    db.prepare(`INSERT INTO ${table}(${columns.join(",")}) VALUES(${columns.map(() => "?").join(",")})`).run(...Object.values(value));
  }
  const postHit = String(post.hit.id), material = repository.prepareReviewMaterial(campaign, 1, postHit);
  assert.ok(material.native_content);
  assert.equal(containsDiscoveryCitation(material, { kind: "native_content", quote: "Milestone achievement for Binance and the Philippines. Game time" }), true);
  db.prepare("DELETE FROM discovery_content_reads WHERE search_hit_id=?").run(postHit);
  assert.equal(new DiscoveryContentRepository(db).nextNative(campaign, "linkedin"), null);
  const columns = Object.keys(post.content);
  db.prepare(`INSERT INTO discovery_content_reads(${columns.join(",")}) VALUES(${columns.map(() => "?").join(",")})`).run(...Object.values(post.content));
  db.prepare("UPDATE discovery_content_reads SET native_content_json=json_set(native_content_json,'$.account_id',?) WHERE search_hit_id=?").run(captured.candidate.platform_account_id!, postHit);
  assert.throws(() => repository.prepareReviewMaterial(campaign, 1, postHit), /content_binding_unavailable/);
});

test("真实模型仅引用Ray当前职位即通过全部要求，审核入库后不再读取帖子", async t => {
  const { db, repository } = await replay(t, true);
  const real = z.object({ operator_account_id: z.string(), input: z.unknown() }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-profile-review-20261020.json", import.meta.url))));
  const receipt = repository.submitReview(real.operator_account_id, real.input);
  assert.equal(receipt.verdict, "qualified");
  assert.equal(db.prepare("SELECT verdict FROM discovery_candidates WHERE id=?").get(candidate)?.verdict, "qualified");
  assert.equal(repository.prepareInitialMatchMaterial(campaign, 1, hit).preserve, true);
  assert.equal(new DiscoveryContentRepository(db).nextNative(campaign, "linkedin"), null);
  assert.equal(repository.getPlan(campaign).used_calls, 0);
});

test("真实Ray联系浮层没有公开联系方式仍完成，普通空网页继续拒绝，完成后不再读取", async t => {
  const { repository, db } = await replay(t, false);
  const real = z.object({ operator_account_id: z.string(), input: z.unknown() }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-profile-review-20261020.json", import.meta.url))));
  repository.submitReview(real.operator_account_id, real.input);
  const actual = parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-empty-contact-20261020.json", import.meta.url)));
  const page = linkedinContactPlan("ray-yee-liew", String(captured.candidate.platform_account_id)).decode(actual);
  const pages = new DiscoveryPublicPageRepository(db);
  const claim = pages.claim({ campaign_id: campaign, expected_revision: 1, candidate_id: candidate, profile_ref: String(captured.hit.profile_ref), requested_url: page.requested_url });
  const { requested_url, ...content } = page;
  const input = { ...content, hidden_email_available: false, stop_code: null, claim };
  assert.throws(() => validateDiscoveryPublicPageResult(input), /attributed content/);
  assert.equal(pages.settle(input).status, "completed");
  assert.equal(pages.next(campaign), null);
  assert.throws(() => pages.claim(claim.input), /attempted/);
  assert.equal(repository.getPlan(campaign).used_calls, 1);
});

test("真实模型对Ex-Binance保留现任雇主缺口，仅此时安排帖子且等待新证据评分", async t => {
  const gap = z.object({ hit: row, author: row, profile: row, candidate: row }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-profile-gap-20261020.json", import.meta.url))));
  const { db, repository } = await replay(t, false, { ...captured, ...gap });
  const real = z.object({ operator_account_id: z.string(), input: z.unknown() }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-gap-review-20261020.json", import.meta.url))));
  const gapHit = String(gap.hit.id);
  assert.equal(new DiscoveryContentRepository(db).nextNative(campaign, "linkedin"), null);
  assert.equal(repository.submitReview(real.operator_account_id, real.input).verdict, "pending");
  assert.equal(repository.prepareInitialMatchMaterial(campaign, 1, gapHit).preserve, false);
  assert.equal(new DiscoveryContentRepository(db).nextNative(campaign, "linkedin"), gapHit);
  db.exec("BEGIN DEFERRED");
  assert.equal(pendingReview(db, campaign, 1, String(gap.candidate.id))?.state, "needs_content");
  db.exec("COMMIT");
  assert.equal(repository.getPlan(campaign).used_calls, 0);
});

test("真实OpenCLI读取分别保留1548位followers和500+connections，拒绝将下界写成精确总数", () => {
  const actual = z.object({ profile: z.unknown() }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-metrics-profile-20261020.json", import.meta.url))));
  const profile = linkedinProfilePlan("ray-yee-liew").decode([actual.profile]);
  assert.equal(profile.follower_count, 1548);
  assert.deepEqual(profile.connections, { raw: "500+connections", lower: 500, upper: null });
  assert.deepEqual(linkedinConnections(profile.connections!.raw), profile.connections);
  assert.throws(() => linkedinProfilePlan("ray-yee-liew").decode([{ ...profile, connections: { ...profile.connections, upper: 500 } }]), /opencli_invalid_output/);
});

for (const logic of ["or", "and"] as const) test(`真实模型审核${logic}：同一份1548 followers、500+ connections资料按完整条件得到不同结论`, () => {
  const actual = z.object({ operator_account_id: z.string(), basis: discoveryReviewBasis, input: z.unknown() }).parse(parseApplicationJson(readFileSync(new URL(`./fixtures/linkedin-metric-${logic}-review-20261020.json`, import.meta.url))));
  const prepared = prepareDiscoveryReview(actual.operator_account_id, actual.input, actual.basis);
  assert.equal(prepared.receipt.verdict, logic === "or" ? "qualified" : "rejected");
  assert.equal(actual.basis.native_followers, 1548);
  assert.deepEqual(actual.basis.native_profile?.connections, { raw: "500+connections", lower: 500, upper: null });
  assert.equal(prepared.receipt.judgments.at(-1)?.outcome, logic === "or" ? "supported" : "contradicted");
});
