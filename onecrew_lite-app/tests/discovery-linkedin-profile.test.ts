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
import { prepareDiscoveryReview, submitDiscoveryReviewInput } from "../src/discovery/review-submission.ts";
import { SELECTION_QUALITY_REVIEWER } from "../src/discovery/quality-policy.ts";
import { buildDiscoveryMatchPrompt, discoveryMatchEvidenceKey, discoveryMatchPromptMaterial, loadBundledDiscoveryMatchRubric } from "../src/discovery/semantic-match.ts";
import { linkedinConnections, linkedinProfilePlan, linkedinContactPlan } from "../src/platforms/opencli/linkedin.ts";
import { DiscoveryPublicPageRepository, validateDiscoveryPublicPageResult } from "../src/discovery/public-page-repository.ts";
import { PLATFORM_READ_FAILURE_CODES } from "../src/platforms/opencli/errors.ts";

const row = z.record(z.string(), z.union([z.string(), z.number(), z.null()]));
const captured = z.object({ plan: row, hit: row, author: row, profile: row, content: row, candidate: row }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-profile-or-post-20261020.json", import.meta.url))));
const campaign = z.string().parse(captured.plan.discovery_campaign_id), hit = z.string().parse(captured.hit.id), candidate = z.string().parse(captured.candidate.id);

// 复用真实历史模型的判断和引文，仅转换已经停用的提交格式。
function separatedCapturedInput(raw: unknown) {
  const input = submitDiscoveryReviewInput.parse(raw), match = input.findings.find(finding => finding.criterion === "match_fit");
  assert.ok(match);
  return { ...input, reviewer: { kind: "agent" as const, label: SELECTION_QUALITY_REVIEWER }, findings: [
    { ...match, criterion: "selection_fit" as const, match_level: null },
    { ...match, criterion: "quality_rating" as const },
    ...input.findings.filter(finding => finding.criterion !== "match_fit"),
  ] };
}

async function replay(t: TestContext, failedContent: boolean, observations = captured, excludedAccounts?: Array<{ native_author_id: string; profile_url: string; excerpt: string }>) {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/linkedin-profile-", import.meta.url))), path = join(directory, "creator.sqlite");
  await initializeNewCreatorDatabase(path);
  const db = new DatabaseSync(path); t.after(() => db.close());
  const repository = new DiscoveryRepository(db);
  const originalBrief = z.string().parse(parseApplicationJson(Buffer.from(String(captured.plan.brief_json))));
  const criteria = z.record(z.string(), z.unknown()).parse(parseApplicationJson(Buffer.from(String(captured.plan.criteria_json))));
  repository.create({ id: campaign, brief: excludedAccounts ? `${originalBrief}\n不要重复读取已经成功的账号。` : originalBrief, opencli_version: "1.8.8", criteria: { ...criteria, ...(excludedAccounts ? { excluded_accounts: excludedAccounts } : {}) }, scopes: parseApplicationJson(Buffer.from(String(captured.plan.scope_json))), target_count: 30 });
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

test("真实LinkedIn已读账号在读取前按已观察主页排除，直接claim不扣预算且不阻断其他作者", async t => {
  const exclusions = [{ native_author_id: String(captured.candidate.platform_account_id), profile_url: String(captured.author.author_url), excerpt: "不要重复读取已经成功的账号。" }];
  const { db, repository } = await replay(t, false, captured, exclusions);
  db.prepare("DELETE FROM discovery_profile_reads WHERE search_hit_id=?").run(hit);
  db.prepare("DELETE FROM discovery_candidates WHERE id=?").run(candidate);
  assert.notEqual(captured.author.author_id, exclusions[0]!.native_author_id);
  const before = repository.getPlan(campaign);
  assert.equal(repository.nextProfileHit(campaign), null);
  assert.throws(() => repository.claimProfile(campaign, 1, hit), /discovery_profile_account_excluded/u);
  assert.deepEqual(repository.getPlan(campaign), before);
  assert.equal(db.prepare("SELECT COUNT(*) AS count FROM discovery_profile_reads").get()!.count, 0);
  const post = z.object({ hit: row, author: row, profile: row }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-post-control-20261020.json", import.meta.url))));
  for (const [table, value] of Object.entries({ discovery_search_hits: post.hit, discovery_search_authors: post.author })) {
    const columns = Object.keys(value);
    db.prepare(`INSERT INTO ${table}(${columns.join(",")}) VALUES(${columns.map(() => "?").join(",")})`).run(...Object.values(value));
  }
  const otherHit = String(post.hit.id);
  const beforePreflight = repository.getPlan(campaign);
  assert.equal(repository.getProfileSource(campaign, otherHit).profile_ref, post.hit.profile_ref);
  assert.throws(() => repository.getProfileSource("other-campaign", otherHit), /discovery_profile_source_unobserved/u);
  assert.deepEqual(repository.getPlan(campaign), beforePreflight);
  assert.equal(db.prepare("SELECT COUNT(*) AS count FROM discovery_profile_reads").get()!.count, 0);
  assert.equal(repository.nextProfileHit(campaign), otherHit);
  const claim = repository.claimProfile(campaign, 1, otherHit);
  assert.equal(claim.call_number, before.used_calls + 1);
  repository.finishProfile(claim, parseApplicationJson(String(post.profile.native_profile_json)), null);
  assert.equal(repository.nextProfileHit(campaign), null);
});

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

test("跨任务复用真实LinkedIn主页，保留采集时间并按新门槛重新判断", async t => {
  const { db, repository } = await replay(t, false);
  const original = repository.getProfile(campaign, hit), nextCampaign = `${campaign}-reuse`, nextHit = `${hit}-reuse`;
  const criteria = z.record(z.string(), z.unknown()).parse(parseApplicationJson(String(captured.plan.criteria_json)));
  repository.create({ id: nextCampaign, brief: `${String(parseApplicationJson(String(captured.plan.brief_json)))}\nLinkedIn 粉丝至少2000000`, opencli_version: "1.8.8", criteria: { ...criteria, min_subscribers: 2000000, min_subscribers_source: "explicit" }, scopes: parseApplicationJson(String(captured.plan.scope_json)), target_count: 30 });
  repository.approve(nextCampaign, 1);
  db.prepare("UPDATE discovery_platform_runs SET status='completed' WHERE discovery_campaign_id=?").run(nextCampaign);
  for (const [table, value] of Object.entries({ discovery_search_hits: { ...captured.hit, id: nextHit, discovery_campaign_id: nextCampaign }, discovery_search_authors: { ...captured.author, search_hit_id: nextHit } })) {
    const columns = Object.keys(value);
    db.prepare(`INSERT INTO ${table}(${columns.join(",")}) VALUES(${columns.map(() => "?").join(",")})`).run(...Object.values(value));
  }
  const claim = repository.claimProfile(nextCampaign, 1, nextHit), reused = repository.reuseProfile(claim);
  assert.ok(reused);
  assert.equal(reused.captured_at, original.captured_at);
  assert.deepEqual(reused.native_profile, original.native_profile);
  assert.deepEqual(repository.getProfile(campaign, hit), original);
  assert.equal(db.prepare("SELECT verdict FROM discovery_candidates WHERE discovery_campaign_id=?").get(nextCampaign)?.verdict, "rejected");
  assert.equal(repository.nextProfileHit(nextCampaign), null);
});

test("真实LinkedIn资料领取后可保存页面超时回执，保留错误码并停止重复读取该账号", async t => {
  const { db, repository } = await replay(t, false);
  db.prepare("DELETE FROM discovery_profile_reads WHERE search_hit_id=?").run(hit);
  db.prepare("DELETE FROM discovery_candidates WHERE id=?").run(candidate);
  const before = repository.getPlan(campaign).used_calls;
  const claim = repository.claimProfile(campaign, 1, hit);
  const receipt = repository.finishProfile(claim, null, "linkedin_profile_timeout");
  assert.equal(receipt.status, "failed");
  assert.equal(receipt.stop_code, "linkedin_profile_timeout");
  assert.equal(repository.getPlan(campaign).used_calls, before + 1);
  assert.equal(repository.getPlan(campaign).platform_runs[0]!.status, "completed");
  assert.equal(repository.nextProfileHit(campaign), null);
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

test("真实旧模型提交被拒绝，既有判断转换独立评分后不再读取帖子", async t => {
  const { db, repository } = await replay(t, true);
  const real = z.object({ operator_account_id: z.string(), input: z.unknown() }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-profile-review-20261020.json", import.meta.url))));
  assert.throws(() => repository.submitReview(real.operator_account_id, real.input), /legacy_match_scoring_retired/);
  const receipt = repository.submitReview(real.operator_account_id, separatedCapturedInput(real.input));
  assert.equal(receipt.verdict, "qualified");
  assert.equal(db.prepare("SELECT verdict FROM discovery_candidates WHERE id=?").get(candidate)?.verdict, "qualified");
  assert.equal(repository.prepareInitialMatchMaterial(campaign, 1, hit).preserve, true);
  assert.equal(new DiscoveryContentRepository(db).nextNative(campaign, "linkedin"), null);
  assert.equal(repository.getPlan(campaign).used_calls, 0);
});

test("真实Ray联系浮层没有公开联系方式仍完成，普通空网页继续拒绝，完成后不再读取", async t => {
  const { repository, db } = await replay(t, false);
  const real = z.object({ operator_account_id: z.string(), input: z.unknown() }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-profile-review-20261020.json", import.meta.url))));
  repository.submitReview(real.operator_account_id, separatedCapturedInput(real.input));
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

test("真实LinkedIn联系领取后保留原始超时分类，失败回执不再次读取且不重复计费", async t => {
  const { repository, db } = await replay(t, false);
  const real = z.object({ operator_account_id: z.string(), input: z.unknown() }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-profile-review-20261020.json", import.meta.url))));
  repository.submitReview(real.operator_account_id, separatedCapturedInput(real.input));
  const pages = new DiscoveryPublicPageRepository(db), input = pages.next(campaign);
  assert.ok(input);
  const claim = pages.claim(input), used = repository.getPlan(campaign).used_calls;
  const failed = { hidden_email_available: false, claim, document_url: null, title: "", markdown_excerpt: "", truncated: false, stop_code: "linkedin_contact_timeout" };
  for (const code of PLATFORM_READ_FAILURE_CODES) assert.equal(validateDiscoveryPublicPageResult({ ...failed, stop_code: code }).stop_code, code);
  const receipt = pages.settle(failed);
  assert.equal(receipt.status, "failed");
  assert.equal(receipt.stop_code, "linkedin_contact_timeout");
  assert.equal(repository.getPlan(campaign).used_calls, used);
  assert.equal(repository.getPlan(campaign).platform_runs[0]!.status, "completed");
  assert.equal(pages.next(campaign), null);
  assert.throws(() => pages.claim(input), /attempted/u);
});

test("真实模型对Ex-Binance保留现任雇主缺口，优先读取Experience且不领取帖子", async t => {
  const gap = z.object({ hit: row, author: row, profile: row, candidate: row }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-profile-gap-20261020.json", import.meta.url))));
  const { db, repository } = await replay(t, false, { ...captured, ...gap });
  const real = z.object({ operator_account_id: z.string(), input: z.unknown() }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-gap-review-20261020.json", import.meta.url))));
  const gapHit = String(gap.hit.id);
  assert.equal(new DiscoveryContentRepository(db).nextNative(campaign, "linkedin"), null);
  assert.equal(repository.submitReview(real.operator_account_id, separatedCapturedInput(real.input)).verdict, "pending");
  assert.equal(repository.prepareInitialMatchMaterial(campaign, 1, gapHit).preserve, false);
  assert.equal(new DiscoveryContentRepository(db).nextNative(campaign, "linkedin"), null);
  assert.equal(repository.nextLinkedinExperience(campaign), gapHit);
  assert.throws(() => repository.claimContent(campaign, 1, gapHit), /discovery_linkedin_experience_required/);
  db.exec("BEGIN DEFERRED");
  assert.equal(pendingReview(db, campaign, 1, String(gap.candidate.id))?.state, "needs_content");
  db.exec("COMMIT");
  assert.equal(repository.getPlan(campaign).used_calls, 0);
});

test("真实LinkedIn候选优先补当前经历且不重读评分作者", async t => {
  const { db, repository } = await replay(t, false);
  const gap = z.object({ hit: row, author: row, profile: row, candidate: row }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-profile-gap-20261020.json", import.meta.url))));
  const post = z.object({ hit: row, author: row, profile: row }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-post-control-20261020.json", import.meta.url))));
  for (const [table, values] of Object.entries({ discovery_search_hits: [gap.hit, post.hit], discovery_search_authors: [gap.author, post.author], discovery_profile_reads: [gap.profile], discovery_candidates: [gap.candidate] })) {
    for (const value of values) {
      const columns = Object.keys(value);
      db.prepare(`INSERT INTO ${table}(${columns.join(",")}) VALUES(${columns.map(() => "?").join(",")})`).run(...Object.values(value));
    }
  }
  const review = z.object({ operator_account_id: z.string(), input: z.unknown() }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-gap-review-20261020.json", import.meta.url))));
  assert.equal(repository.submitReview(review.operator_account_id, separatedCapturedInput(review.input)).verdict, "pending");
  const unread = String(post.hit.id), gapHit = String(gap.hit.id), expected = { kind: "read_content", search_hit_id: gapHit };
  assert.equal(repository.nextContentHit(campaign, "linkedin"), gapHit);
  assert.deepEqual(repository.planStep(campaign, 1), expected);
  assert.deepEqual(repository.planStepScoring(campaign, 1, [], [], [hit]), expected);
  assert.deepEqual(repository.planStep(campaign, 1, [], [], { holdDiscovery: true }), expected);
  assert.deepEqual(repository.planStepScoring(campaign, 1, [], [], [hit], { holdDiscovery: true }), expected);
  assert.equal(repository.nextIndependentRead(campaign, 1, hit, [hit, gapHit], { holdDiscovery: true }), null);
  assert.deepEqual(repository.planStepScoring(campaign, 1, [], [], [hit, gapHit], { holdDiscovery: true }), { kind: "busy" });
  assert.equal(repository.nextProfileHit(campaign), unread);
  assert.equal(repository.getPlan(campaign).used_calls, 0);
  const claim = repository.claimProfile(campaign, 1, unread);
  repository.finishProfile(claim, parseApplicationJson(String(post.profile.native_profile_json)), null);
  assert.equal(repository.nextProfileHit(campaign), null);
  assert.deepEqual(repository.planStep(campaign, 1), { kind: "read_content", search_hit_id: gapHit });
  assert.deepEqual(repository.nextIndependentRead(campaign, 1, hit, [hit]), { kind: "read_content", search_hit_id: gapHit });
  assert.equal(repository.getPlan(campaign).used_calls, 1);
});

test("真实OpenCLI读取分别保留1548位followers和500+connections，拒绝将下界写成精确总数", () => {
  const actual = z.object({ profile: z.unknown() }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-metrics-profile-20261020.json", import.meta.url))));
  const profile = linkedinProfilePlan("ray-yee-liew").decode([actual.profile]);
  assert.equal(profile.follower_count, 1548);
  assert.deepEqual(profile.connections, { raw: "500+connections", lower: 500, upper: null });
  assert.deepEqual(linkedinConnections(profile.connections!.raw), profile.connections);
  assert.throws(() => linkedinProfilePlan("ray-yee-liew").decode([{ ...profile, connections: { ...profile.connections, upper: 500 } }]), /opencli_invalid_output/);
});

test("真实Dylan经历补读结束后从活动等待排除，失败任务排除表独立生效", async t => {
  const fixture = z.object({ rows: z.record(z.string(), row) }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-experience-dylan-rows-20261021.json", import.meta.url))));
  const live = z.object({ native: z.unknown() }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-experience-live-dylan-20261021.json", import.meta.url))));
  const materials = z.object({ records: z.array(z.object({ material: z.object({ campaign_id: z.string(), criteria: z.unknown(), approved_scopes: z.unknown() }) })) }).parse(parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-occupational-headlines-20261021.json", import.meta.url))));
  const material = materials.records[0]!.material;
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/linkedin-active-pending-", import.meta.url))), path = join(directory, "creator.sqlite");
  await initializeNewCreatorDatabase(path);
  const db = new DatabaseSync(path); t.after(() => db.close());
  const repository = new DiscoveryRepository(db), campaign = material.campaign_id, hit = String(fixture.rows.discovery_search_hits!.id);
  repository.create({ id: campaign, brief: "在LinkedIn搜索币安、OKX、Bitget、Bybit或Gate.io的商务、BD或Marketing人员。成功的账号不重复读取。币安（Binance）、OKX、Bitget、Bybit、Gate.io 的商务、BD 或 Marketing 人员", opencli_version: "1.8.8", criteria: material.criteria, scopes: material.approved_scopes, target_count: 30 });
  repository.approve(campaign, 1);
  db.prepare("UPDATE discovery_platform_runs SET status='completed' WHERE discovery_campaign_id=?").run(campaign);
  db.prepare("UPDATE discovery_campaigns SET used_calls=70 WHERE id=?").run(campaign);
  for (const [table, value] of Object.entries(fixture.rows)) {
    const columns = Object.keys(value);
    db.prepare(`INSERT INTO ${table}(${columns.join(",")}) VALUES(${columns.map(() => "?").join(",")})`).run(...Object.values(value));
  }
  assert.equal(repository.activePendingCount(campaign, 1), 1);
  assert.equal(repository.activePendingCount(campaign, 1, [], [hit]), 0);
  const claim = repository.claimLinkedinExperience(campaign, 1, hit);
  repository.finishLinkedinExperience(claim, live.native, null);
  db.exec("BEGIN DEFERRED");
  assert.equal(pendingReview(db, campaign, 1, String(fixture.rows.discovery_candidates!.id))?.state, "insufficient_after_read");
  db.exec("COMMIT");
  assert.equal(repository.activePendingCount(campaign, 1), 0);
  assert.equal(repository.nextLinkedinExperience(campaign), null);
});

for (const logic of ["or", "and"] as const) test(`真实模型审核${logic}：同一份1548 followers、500+ connections资料按完整条件得到不同结论`, () => {
  const actual = z.object({ operator_account_id: z.string(), basis: discoveryReviewBasis, input: z.unknown() }).parse(parseApplicationJson(readFileSync(new URL(`./fixtures/linkedin-metric-${logic}-review-20261020.json`, import.meta.url))));
  const input = separatedCapturedInput(actual.input);
  if (logic === "and") {
    const contradicted = input.findings.find(finding => typeof finding.criterion === "object" && finding.outcome === "contradicted");
    assert.ok(contradicted);
    input.findings[0] = { ...contradicted, criterion: "selection_fit", match_level: null };
  }
  const prepared = prepareDiscoveryReview(actual.operator_account_id, input, actual.basis);
  assert.equal(prepared.receipt.verdict, logic === "or" ? "qualified" : "rejected");
  assert.equal(actual.basis.native_followers, 1548);
  assert.deepEqual(actual.basis.native_profile?.connections, { raw: "500+connections", lower: 500, upper: null });
  assert.equal(prepared.receipt.judgments.at(-1)?.outcome, logic === "or" ? "supported" : "contradicted");
});
