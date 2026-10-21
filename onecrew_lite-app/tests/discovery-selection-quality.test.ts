import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { initializeNewCreatorDatabase } from "../src/storage/creator-schema.ts";
import { DiscoveryRepository } from "../src/discovery/repository.ts";
import { discoveryMatchProposal } from "../src/discovery/semantic-match.ts";
import { SELECTION_QUALITY_REVIEWER, evaluateQualityFilter, validateProposedQualityFilter, freezeQualityFilter, type ProposedQualityFilter } from "../src/discovery/quality-policy.ts";
import { assessDiscoveryProfile, assessedDiscoveryEvidence } from "../src/discovery/qualification.ts";
import { discoveryQualityScore, type DiscoveryQualityLevel } from "@onecrew/runtime-contracts/discovery";
import { discoveryPlanInput } from "../src/discovery/contracts.ts";

const saved = parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-profile-or-post-20261020.json", import.meta.url))) as Record<"plan" | "hit" | "author" | "profile" | "candidate", Record<string, string | number | null>>;
const capturedReview = parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-selection-quality-review-20261020.json", import.meta.url))) as Record<string, unknown>;
const review = discoveryMatchProposal.parse({ selection: capturedReview.selection, quality_rating: capturedReview.quality_rating, language_only_pending: capturedReview.language_only_pending, requirement_findings: capturedReview.requirement_findings });

test("真实独立匹配与质量回执保存85分，重启读取保持证据且不增加平台读取", async t => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/quality-regression-", import.meta.url))), path = join(directory, "creator.sqlite");
  await initializeNewCreatorDatabase(path);
  const db = new DatabaseSync(path); t.after(() => db.close());
  const repository = new DiscoveryRepository(db), campaign = String(saved.plan.discovery_campaign_id), hit = String(saved.hit.id);
  const criteria = { ...parseApplicationJson(Buffer.from(String(saved.plan.criteria_json))) as Record<string, unknown>, quality_filter: null };
  repository.create({ id: campaign, brief: parseApplicationJson(Buffer.from(String(saved.plan.brief_json))), opencli_version: "1.8.8", criteria, scopes: parseApplicationJson(Buffer.from(String(saved.plan.scope_json))), target_count: 30, max_calls: null });
  repository.approve(campaign, 1);
  db.prepare("UPDATE discovery_campaign_revisions SET approved_at=? WHERE discovery_campaign_id=?").run(saved.plan.approved_at!, campaign);
  db.prepare("UPDATE discovery_platform_runs SET status='completed' WHERE discovery_campaign_id=?").run(campaign);
  for (const [table, row] of Object.entries({ discovery_search_hits: saved.hit, discovery_search_authors: saved.author, discovery_profile_reads: saved.profile, discovery_candidates: saved.candidate })) {
    const keys = Object.keys(row); db.prepare(`INSERT INTO ${table}(${keys.join(",")}) VALUES(${keys.map(() => "?").join(",")})`).run(...Object.values(row));
  }
  const material = repository.prepareReviewMaterial(campaign, 1, hit);
  const input = { campaign_id: campaign, expected_revision: 1, search_hit_id: hit, material_hash: material.material_hash, request_id: "real-selection-quality-regression", reviewer: { kind: "agent", label: SELECTION_QUALITY_REVIEWER }, findings: [review.selection, review.quality_rating, ...review.requirement_findings] };
  assert.throws(() => repository.submitReview("quality-regression", { ...input, findings: [review.selection, ...review.requirement_findings] }), /selection_quality_findings_required/);
  assert.throws(() => repository.submitReview("quality-regression", { ...input, material_hash: "0".repeat(64) }), /material/);
  const badCitation = { ...review.quality_rating, citations: [{ ...review.quality_rating.citations[0]!, quote: "not present in saved profile" }] };
  assert.throws(() => repository.submitReview("quality-regression", { ...input, findings: [review.selection, badCitation, ...review.requirement_findings] }), /citation/);
  const receipt = repository.submitReview("quality-regression", input);
  assert.equal(receipt.verdict, "qualified");
  const stored = db.prepare("SELECT evidence_json FROM discovery_candidates WHERE id=?").get(saved.candidate.id!);
  const evidence = parseApplicationJson(Buffer.from(String(stored!.evidence_json))) as Record<string, unknown>;
  const selected = evidence.reviewed_selection as Record<string, unknown>, quality = evidence.reviewed_quality as Record<string, unknown>;
  assert.equal(selected.outcome, "supported"); assert.equal(quality.score, 85);
  assert.equal(new DiscoveryRepository(db).prepareInitialMatchMaterial(campaign, 1, hit).preserve, true);
  assert.equal(repository.getPlan(campaign).used_calls, 0);
  const gap = parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-profile-gap-20261020.json", import.meta.url))) as Record<"hit" | "author" | "profile" | "candidate", Record<string, string | number | null>>;
  for (const [table, row] of Object.entries({ discovery_search_hits: gap.hit, discovery_search_authors: gap.author, discovery_profile_reads: gap.profile, discovery_candidates: gap.candidate })) {
    const keys = Object.keys(row); db.prepare(`INSERT INTO ${table}(${keys.join(",")}) VALUES(${keys.map(() => "?").join(",")})`).run(...Object.values(row));
  }
  const contact = repository.planStepScoring(campaign, 1, [], ["linkedin", "google", "yahoo"], [String(gap.hit.id)]);
  assert.equal(contact.kind, "read_public_page");
  if (contact.kind === "read_public_page") assert.equal(contact.candidate_id, saved.candidate.id);
  const heldContact = repository.planStepScoring(campaign, 1, [], ["linkedin", "google", "yahoo"], [hit]);
  assert.notEqual(heldContact.kind, "read_public_page");
  assert.notEqual(repository.planStepScoring(campaign, 1, [], [], [hit]).kind, "read_public_page");
  assert.notEqual(repository.planStepScoring(campaign, 1, ["linkedin"], ["linkedin"], [String(gap.hit.id)]).kind, "read_public_page");
  assert.equal(repository.getPlan(campaign).used_calls, 0);
  const author = String(saved.candidate.platform_account_id);
  for (const level of ["excellent", "good", "medium", "low"] as DiscoveryQualityLevel[]) {
    const policyInput = { ...evidence, reviewed_quality: { ...quality, level, score: discoveryQualityScore(level) } };
    const assessment = assessDiscoveryProfile(criteria, policyInput, author, 1548);
    assert.equal(assessment.verdict, "qualified", level);
    const projected = assessedDiscoveryEvidence(policyInput, assessment) as { assessment: { quality_rating: { score: number } } };
    assert.equal(projected.assessment.quality_rating.score, discoveryQualityScore(level));
    assert.equal(assessDiscoveryProfile({ ...criteria, quality_filter: { kind: "minimum", operator: "gte", score: 85, excerpt: "评分至少85" } }, policyInput, author, 1548).verdict, ["excellent", "good"].includes(level) ? "qualified" : "rejected");
  }
  const unrated = { ...evidence, reviewed_quality: { ...quality, outcome: "insufficient", level: null, score: null } };
  assert.equal(assessDiscoveryProfile(criteria, unrated, author, 1548).verdict, "qualified");
  for (const [outcome, verdict] of [["contradicted", "rejected"], ["insufficient", "pending"]] as const) {
    assert.equal(assessDiscoveryProfile(criteria, { ...evidence, reviewed_selection: { ...selected, outcome } }, author, 1548).verdict, verdict);
  }
});

test("明确评分门槛按实际代表分比较，或者与同时保留三值逻辑", () => {
  for (const [operator, score, level, expected] of [["gte",85,"good","supported"],["gt",85,"good","contradicted"],["gte",80,"good","supported"],["gte",95,"excellent","supported"],["gt",95,"excellent","contradicted"],["gte",100,"excellent","contradicted"],["gte",40,null,"insufficient"]] as const) {
    assert.equal(evaluateQualityFilter({ kind: "minimum", operator, score, excerpt: "评分" }, level, []), expected);
  }
  const minimum = { kind: "minimum", operator: "gte", score: 85, excerpt: "评分至少85" } as const;
  for (const kind of ["all", "any"] as const) {
    const rule = { kind, items: [minimum, { kind: "requirement", index: 0 } as const] };
    assert.equal(evaluateQualityFilter(rule, "low", ["supported"]), kind === "any" ? "supported" : "contradicted");
    assert.equal(evaluateQualityFilter(rule, "good", ["insufficient"]), kind === "any" ? "supported" : "insufficient");
  }
});

test("评分门槛必须来自用户明确要求，偏好、人数和显示请求不会成为筛选条件", () => {
  for (const [request, score, accepted] of [["评分至少85",85,true],["至少良好",85,true],["最低80分",80,true],["评分最低80分",80,true],["最好评分85",85,false],["显示评分95",95,false],["找85人",85,false],["followers至少2000",85,false]] as const) {
    assert.equal(validateProposedQualityFilter(request, { kind: "minimum", operator: "gte", score, excerpt: request }), accepted, request);
  }
  const requirements: Array<{ description: string; excerpt: string }> = [];
  const proposed: ProposedQualityFilter = { kind: "any", items: [{ kind: "minimum", operator: "gte", score: 85, excerpt: "评分至少85" }, { kind: "condition", description: "任职币安", excerpt: "任职币安" }] };
  assert.equal(validateProposedQualityFilter("评分至少85或者任职币安", proposed), true);
  assert.deepEqual(freezeQualityFilter(proposed, requirements), { kind: "any", items: [proposed.items[0], { kind: "requirement", index: 0 }] });
  const base = { id: "quality-criteria", brief: "找85人", opencli_version: "1.8.8", criteria: { quality_filter: { kind: "minimum", operator: "gte", score: 85, excerpt: "找85人" } }, scopes: [{ platform: "linkedin", profile_ref: "default", keywords: ["BD"] }], target_count: 85, max_calls: null };
  assert.equal(discoveryPlanInput.safeParse(base).success, false);
});

test("真实模型的一句话评分或任职条件通过计划合同并保留任选关系", () => {
  const actual = parseApplicationJson(readFileSync(new URL("./fixtures/linkedin-quality-intent-20261020.json", import.meta.url))) as { draft: unknown };
  const draft = discoveryPlanInput.parse(actual.draft);
  assert.equal(draft.criteria.min_subscribers, 0);
  assert.deepEqual(draft.criteria.quality_filter, { kind: "any", items: [{ kind: "minimum", operator: "gte", score: 85, excerpt: "评分至少85" }, { kind: "requirement", index: 0 }] });
});
