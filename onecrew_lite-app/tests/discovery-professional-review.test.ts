import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import { z } from "zod";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { applyDiscoveryProfessionalReview, buildDiscoveryProfessionalReviewPrompt, discoveryProfessionalReview } from "../src/discovery/professional-review.ts";
import { buildDiscoveryMatchPrompt, discoveryMatchProposal, discoveryMatchPromptMaterial, loadBundledDiscoveryMatchRubric } from "../src/discovery/semantic-match.ts";
import { discoveryExecutionFailureCode, discoveryScoreDeferred } from "../src/discovery/execution-failure.ts";
import type { DiscoveryReviewMaterial } from "../src/discovery/review-material.ts";

const captured = z.object({ objective: z.string(), records: z.array(z.object({ name: z.string(), material: z.record(z.string(), z.unknown()), previous_findings: discoveryMatchProposal.shape.requirement_findings })) }).parse(parseApplicationJson(await readFile(new URL("./fixtures/linkedin-professional-materials-20261021.json", import.meta.url))));
const results = z.object({ results: z.array(z.object({ name: z.string(), audit: discoveryProfessionalReview, expected: z.string() })) }).parse(parseApplicationJson(await readFile(new URL("./fixtures/linkedin-professional-reviews-20261021.json", import.meta.url))));
function sample(name: string) {
  const record = captured.records.find(value => value.name === name)!;
  const proposal = discoveryMatchProposal.parse({ selection: { ...record.previous_findings.find(value => value.criterion === "selection_fit"), match_level: null }, quality_rating: record.previous_findings.find(value => value.criterion === "quality_rating"), language_only_pending: false, requirement_findings: record.previous_findings.filter(value => typeof value.criterion === "object") });
  return { material: record.material as DiscoveryReviewMaterial, proposal, audit: structuredClone(results.results.find(value => value.name === name)!.audit) };
}

test("真实已保存LinkedIn资料独立复核：保留充分任职证据，技能、联盟和时间冲突保持待核实", () => {
  for (const result of results.results) {
    const { material, proposal, audit } = sample(result.name), before = structuredClone(proposal);
    const checked = applyDiscoveryProfessionalReview(material, proposal, audit);
    assert.equal([checked.selection, ...checked.requirement_findings].every(value => value.outcome === "supported") ? "supported" : "insufficient", result.expected, result.name);
    assert.deepEqual(checked.quality_rating, before.quality_rating);
    assert.deepEqual(proposal, before);
  }
});

test("职业复核不接受缺项、重复条件或无来源引文，不升级原有待核实结论", () => {
  const { material, proposal, audit } = sample("Nuaman Khan");
  assert.throws(() => applyDiscoveryProfessionalReview(material, proposal, { ...audit, checks: audit.checks.slice(1) }), /initial_match_professional_review_invalid/);
  assert.throws(() => applyDiscoveryProfessionalReview(material, proposal, { ...audit, checks: [...audit.checks, audit.checks[0]] }), /initial_match_professional_review_invalid/);
  const invalid = structuredClone(audit);
  invalid.checks[0]!.citations = [{ kind: "native_profile", profile_url: material.native_profile!.profile_url, quote: "未在原始资料出现的引文" }];
  assert.throws(() => applyDiscoveryProfessionalReview(material, proposal, invalid), /initial_match_professional_review_invalid/);
  const pending = { ...proposal, selection: { ...proposal.selection, outcome: "insufficient" as const } };
  assert.equal(applyDiscoveryProfessionalReview(material, pending, audit).selection.outcome, "insufficient");
  assert.equal(discoveryExecutionFailureCode(new Error("initial_match_professional_review_invalid")), "initial_match_professional_review_invalid");
  assert.equal(discoveryScoreDeferred("initial_match_professional_review_invalid"), true);
});

test("职业复核按请求区分现任和曾任，技能和外部合作不能满足现任岗位", () => {
  const { material, proposal, audit } = sample("Nuaman Khan");
  const role = audit.checks.find(value => value.criterion === "selection_fit")!;
  role.role_required = true; role.role_evidence = "skills_only";
  assert.equal(applyDiscoveryProfessionalReview(material, proposal, audit).selection.outcome, "insufficient");
  role.role_evidence = "position_title";
  const employer = audit.checks.find(value => typeof value.criterion === "object")!;
  employer.employment_required = true; employer.time_requirement = "current"; employer.relationship = "external_partner";
  assert.equal(applyDiscoveryProfessionalReview(material, proposal, audit).requirement_findings[0]!.outcome, "insufficient");
  employer.time_requirement = "past"; employer.relationship = "past_staff"; employer.time_relation = "past";
  assert.equal(applyDiscoveryProfessionalReview(material, proposal, audit).requirement_findings[0]!.outcome, "supported");
});

test("真实39人排除名单由应用执行，模型保留本人的全部证据和筛选条件", async () => {
  const { material, proposal } = sample("Nuaman Khan"), original = structuredClone(material);
  const rubric = await loadBundledDiscoveryMatchRubric();
  const prompt = buildDiscoveryMatchPrompt(captured.objective, material, discoveryMatchPromptMaterial(material, null), rubric);
  const model = z.object({ material: z.record(z.string(), z.unknown()) }).parse(parseApplicationJson(Buffer.from(prompt.split("UNTRUSTED_SAVED_DATA:\n")[1]!))).material;
  const criteria = z.record(z.string(), z.unknown()).parse(model.criteria);
  const originalCriteria = z.record(z.string(), z.unknown()).parse(material.criteria);
  assert.equal(z.array(z.unknown()).parse(originalCriteria.excluded_accounts).length, 39);
  assert.equal(criteria.excluded_accounts, undefined);
  assert.equal(model.account_exclusion_enforced_by_application, true);
  assert.deepEqual(criteria.requirements, originalCriteria.requirements);
  assert.deepEqual(model.native_profile, material.native_profile);
  assert.deepEqual(material, original);
  const auditPrompt = buildDiscoveryProfessionalReviewPrompt(captured.objective, material);
  assert.equal(auditPrompt.includes(proposal.selection.rationale), false);
  assert.ok(auditPrompt.includes("OR"));
  assert.ok(auditPrompt.includes("AND"));
});
