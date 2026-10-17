import { randomUUID } from "node:crypto";
import { z } from "zod";
import { applyDiscoveryRequirements } from "./review-requirements.ts";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { discoveryJsonValue, discoveryMaterialDigest } from "./library-material.ts";
import { applyInitialDiscoveryMatch, discoveryReviewFinding, INITIAL_MATCH_POLICY, INITIAL_MATCH_REVIEWER, type DiscoveryReviewFinding } from "./initial-match.ts";
import { applyDiscoveryReviewFindings } from "./review-findings.ts";
import { buildDiscoveryReviewMaterial, type DiscoveryReviewBasis } from "./review-material.ts";
import { assessDiscoveryDouyin, assessDiscoveryFacebook, assessDiscoveryProfile, assessDiscoveryYoutube, assessedDiscoveryEvidence } from "./qualification.ts";

const text = z.string().refine(value => value.isWellFormed());
const bounded = (maximum: number) => text.refine(value => !!value.trim() && Buffer.byteLength(value) <= maximum);
const unsigned = z.union([z.number().int().nonnegative().safe(), z.bigint().min(0n).max(18446744073709551615n)]);
export const discoveryReviewer = z.strictObject({ kind: z.enum(["agent", "human"]), label: bounded(256) });
export const submitDiscoveryReviewInput = z.strictObject({ campaign_id: bounded(128), expected_revision: z.number().int().positive().max(4294967295), search_hit_id: bounded(128), material_hash: text.regex(/^[a-f0-9]{64}$/u), request_id: bounded(128), reviewer: discoveryReviewer, findings: z.array(discoveryReviewFinding).max(40) });
export type SubmitDiscoveryReviewInput = z.infer<typeof submitDiscoveryReviewInput>;
export const discoveryReviewReceipt = z.strictObject({ review_id: text, operator_account_id: text, campaign_id: text, criteria_revision: z.number().int().positive().max(4294967295), search_hit_id: text, candidate_id: text, native_author_id: text, profile_ref: text, request_id: text, material_hash: text, policy_version: text, judgments: z.array(z.strictObject({ criterion: discoveryReviewFinding.shape.criterion, outcome: discoveryReviewFinding.shape.outcome })), verdict: text, reasons: z.array(text), subscribers: unsigned.nullable(), created_at: z.union([z.number().int().safe(), z.bigint().min(-9223372036854775808n).max(9223372036854775807n)]) });
export type DiscoveryReviewReceipt = z.infer<typeof discoveryReviewReceipt>;
export interface PreparedDiscoveryReview { basis: DiscoveryReviewBasis; request_hash: string; material_json: string; reviewer_json: string; findings_json: string; receipt_json: string; evidence_json: string; receipt: DiscoveryReviewReceipt }
function encoded(value: unknown, maximum: number): string { const json = stringifyApplicationJson(value); if (Buffer.byteLength(json) > maximum) throw new Error("review_input_too_large"); return json; }
function findingValue(finding: DiscoveryReviewFinding) {
  return { criterion: finding.criterion, outcome: finding.outcome, rationale: finding.rationale, citations: finding.citations, tool: finding.tool ?? null, ...(finding.country_code != null ? { country_code: finding.country_code } : {}), ...(finding.match_level != null ? { match_level: finding.match_level } : {}) };
}
function inputValue(input: SubmitDiscoveryReviewInput) { return { campaign_id: input.campaign_id, expected_revision: input.expected_revision, search_hit_id: input.search_hit_id, material_hash: input.material_hash, request_id: input.request_id, reviewer: input.reviewer, findings: input.findings.map(findingValue) }; }
export function discoveryReviewRequestHash(account: string, raw: unknown): string {
  bounded(256).parse(account); const input = submitDiscoveryReviewInput.parse(raw);
  const value = ["youtube-text-review-v1", account, inputValue(input)]; encoded(value, 65536); return discoveryMaterialDigest(value);
}

export function prepareDiscoveryReview(account: string, raw: unknown, basis: DiscoveryReviewBasis): PreparedDiscoveryReview {
  const input = submitDiscoveryReviewInput.parse(raw), requestHash = discoveryReviewRequestHash(account, input), material = buildDiscoveryReviewMaterial(basis);
  if (input.campaign_id !== basis.campaign_id || input.expected_revision !== basis.revision || input.search_hit_id !== basis.hit.id || input.material_hash !== material.material_hash) throw new Error("review_material_changed");
  const reviewId = randomUUID(), initial = input.reviewer.kind === "agent" && input.reviewer.label === INITIAL_MATCH_REVIEWER;
  const saved = parseApplicationJson(Buffer.from(basis.evidence_json)), evidence = initial ? applyInitialDiscoveryMatch(material, input.findings, saved, reviewId) : applyDiscoveryReviewFindings(material, input.findings, saved, reviewId);
  applyDiscoveryRequirements(material, input.findings, evidence, reviewId, input.expected_revision);
  let assessment;
  if (material.hit.platform === "douyin" && material.native_follower_estimate) assessment = assessDiscoveryDouyin(material.criteria, evidence, material.native_author_id, material.native_follower_estimate.display);
  else if (material.hit.platform === "facebook" && material.native_follower_estimate) assessment = assessDiscoveryFacebook(material.criteria, evidence, material.native_author_id, material.native_follower_estimate.display);
  else if (["reddit", "facebook", "pinterest", "linkedin", "instagram", "weibo"].includes(material.hit.platform)) assessment = assessDiscoveryProfile(material.criteria, evidence, material.native_author_id, material.native_followers ?? null);
  else if (["twitter", "tiktok", "bilibili"].includes(material.hit.platform)) { if (material.native_followers === undefined) throw new Error("review_native_followers_unavailable"); assessment = assessDiscoveryProfile(material.criteria, evidence, material.native_author_id, material.native_followers); }
  else assessment = assessDiscoveryYoutube(material.criteria, evidence, material.native_author_id);
  const criteria = material.criteria !== null && typeof material.criteria === "object" && !Array.isArray(material.criteria) ? material.criteria as Record<string, unknown> : {};
  const policy = initial ? INITIAL_MATCH_POLICY : input.findings.some(finding => typeof finding.criterion === "string" && ["content_language", "match_fit"].includes(finding.criterion)) ? "language-match-review-v1" : typeof criteria.category === "string" && !!criteria.category && criteria.category !== "服装" ? "category-content-review-v1" : criteria.category === "服装" || criteria.region === "中国大陆以外" ? ["twitter", "tiktok"].includes(material.hit.platform) ? "apparel-native-review-v2" : "apparel-residence-review-v1" : "youtube-text-review-v1";
  const now = Math.floor(Date.now() / 1000); if (!Number.isSafeInteger(now) || now < 0) throw new Error("review_clock_invalid");
  const receipt = discoveryReviewReceipt.parse({ review_id: reviewId, operator_account_id: account, campaign_id: basis.campaign_id, criteria_revision: basis.revision, search_hit_id: basis.hit.id, candidate_id: basis.candidate_id, native_author_id: basis.native_author_id, profile_ref: basis.hit.profile_ref, request_id: input.request_id, material_hash: material.material_hash, policy_version: policy, judgments: input.findings.map(finding => ({ criterion: finding.criterion, outcome: finding.outcome })), verdict: assessment.verdict, reasons: assessment.reasons, subscribers: assessment.subscribers, created_at: now });
  return { basis, request_hash: requestHash, material_json: encoded(material, 1048576), reviewer_json: encoded(input.reviewer, 2048), findings_json: encoded(input.findings.map(findingValue), 65536), receipt_json: encoded(receipt, 16384), evidence_json: encoded(discoveryJsonValue(assessedDiscoveryEvidence(evidence, assessment)), 1048576), receipt };
}
