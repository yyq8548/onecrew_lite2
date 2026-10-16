import { z } from "zod";
import { discoveryReviewCriterion } from "@onecrew/runtime-contracts/discovery";
import { containsDiscoveryCitation, matchedDiscoveryLanguageSource, type DiscoveryReviewMaterial } from "./review-material.ts";

export const INITIAL_MATCH_REVIEWER = "onecrew-match-evaluation-0cebf122-initial-v1";
export const INITIAL_MATCH_POLICY = "initial-match-evaluation-v1";
const text = z.string().refine(value => value.isWellFormed());
const quote = z.discriminatedUnion("kind", [
  z.strictObject({ kind: z.literal("description"), quote: text }), z.strictObject({ kind: z.literal("search_caption"), quote: text }), z.strictObject({ kind: z.literal("native_content"), quote: text }),
  z.strictObject({ kind: z.literal("transcript"), segment_index: z.number().int().min(0).max(4294967295), quote: text }),
  z.strictObject({ kind: z.literal("native_profile"), profile_url: text, quote: text }), z.strictObject({ kind: z.literal("public_page"), requested_url: text, quote: text }),
]);
export const discoveryReviewFinding = z.strictObject({ criterion: discoveryReviewCriterion, outcome: z.enum(["supported", "contradicted", "insufficient"]), rationale: text, citations: z.array(quote), tool: z.strictObject({ name: text, requested_url: text }).nullish(), country_code: text.nullish(), match_level: z.enum(["excellent", "good", "medium", "low"]).nullish() });
export type DiscoveryReviewFinding = z.infer<typeof discoveryReviewFinding>;
const object = (value: unknown): Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
const u64 = (value: unknown): bigint | null => typeof value === "bigint" && value >= 0n && value <= 18446744073709551615n ? value : typeof value === "number" && Number.isSafeInteger(value) && value >= 0 ? BigInt(value) : null;

function validateAudienceLanguage(material: DiscoveryReviewMaterial, finding: DiscoveryReviewFinding) {
  if (finding.outcome !== "supported" || finding.match_level === "low") return;
  const samples = material.language_samples ?? [], native = object(material.native_content?.native_content), original = native[material.hit.platform === "bilibili" ? "description" : "text"];
  const audienceClaim = finding.rationale.includes("粉丝评论语言符合") || finding.rationale.includes("Audience comment language matches");
  const commentOnly = finding.citations.some(citation => citation.kind === "native_content" && samples.some(sample => sample.comments.some(comment => comment.text.includes(citation.quote))) && !samples.some(sample => sample.text.includes(citation.quote)) && !(typeof original === "string" && original.includes(citation.quote)));
  if (!audienceClaim && !commentOnly) return;
  const language = object(material.criteria).language;
  if (typeof language === "string" && ["spoken", "speech", "口语", "口播", "语音"].some(word => language.toLowerCase().includes(word)) || !audienceClaim || finding.match_level !== "good") throw new Error("initial_match_audience_language_invalid");
  const source = matchedDiscoveryLanguageSource(material);
  const enough = samples.some(sample => {
    if (sample.author_id !== material.native_author_id || sample.source_url !== source) return false;
    const texts = new Set<string>(), sources = new Set<string>(); let cited = 0;
    for (const comment of sample.comments.slice(0, 5)) {
      const value = comment.text.trim();
      if ([...value].filter(character => /\p{Alphabetic}/u.test(character)).length < 3 || value.startsWith("https://") || value.startsWith("http://")) continue;
      const normalized = value.toLowerCase(); if (texts.has(normalized)) continue; texts.add(normalized);
      if (sources.has(comment.source_url)) continue; sources.add(comment.source_url);
      if (finding.citations.some(citation => citation.kind === "native_content" && citation.quote.trim() === value)) cited++;
    }
    return cited >= 3;
  });
  if (!enough) throw new Error("initial_match_audience_sample_insufficient");
}

// 仅验证已冻结材料中的判断；持久审核收据和账号身份由仓储事务另行提交。
export function applyInitialDiscoveryMatch(material: DiscoveryReviewMaterial, rawFindings: unknown, rawEvidence: unknown, reviewId: string): Record<string, unknown> {
  const findings = z.array(discoveryReviewFinding).parse(rawFindings).filter(finding => typeof finding.criterion === "string"), finding = findings[0];
  if (!finding || findings.length > 2) throw new Error("initial_match_single_finding_required");
  const criteria = object(material.criteria), evidence = object(rawEvidence), language = criteria.language, secondary = findings[1];
  if (secondary && (finding.outcome !== "insufficient" || secondary.criterion !== "content_language" || secondary.outcome !== "insufficient" || secondary.match_level != null || secondary.tool != null || secondary.country_code != null || !secondary.rationale.trim() || Buffer.byteLength(secondary.rationale) > 4000 || secondary.citations.length > 8 || !(typeof language === "string" && language.trim()) || !secondary.citations.every(citation => containsDiscoveryCitation(material, citation)))) throw new Error("initial_match_language_request_invalid");
  const supported = finding.outcome === "supported";
  if (finding.criterion !== "match_fit" || finding.outcome === "contradicted" || supported !== (finding.match_level != null) || !finding.rationale.trim() || Buffer.byteLength(finding.rationale) > 4000 || finding.tool != null || finding.country_code != null || finding.citations.length > 8 || !finding.citations.every(citation => containsDiscoveryCitation(material, citation))) throw new Error("initial_match_finding_invalid");
  const rawMinimum = u64(criteria.min_subscribers) ?? u64(criteria.min_followers), minimum = rawMinimum !== null && rawMinimum > 0n ? rawMinimum : null;
  const count = u64(material.native_followers) ?? u64(object(evidence.assessment).subscribers_normalized), estimate = material.native_follower_estimate;
  const below = minimum !== null && (estimate ? BigInt(estimate.upper) < minimum : count !== null && count < minimum);
  const unknown = estimate ? minimum !== null && BigInt(estimate.lower) < minimum && BigInt(estimate.upper) >= minimum : count === null;
  if (secondary && minimum !== null && unknown) throw new Error("initial_match_language_request_invalid");
  if (below && finding.match_level !== "low") throw new Error("initial_match_required_failure_must_be_low");
  if (!below && minimum !== null && unknown && supported && finding.match_level !== "low") throw new Error("initial_match_required_count_unknown");
  if (supported && !below && !finding.citations.some(citation => ["description", "search_caption", "transcript", "native_content", "native_profile"].includes(citation.kind))) throw new Error("initial_match_author_evidence_required");
  if (supported && !below && finding.match_level !== "low" && typeof language === "string" && language.trim() && finding.citations.some(citation => citation.kind === "native_profile") && !finding.citations.some(citation => ["description", "search_caption", "transcript", "native_content"].includes(citation.kind))) throw new Error("initial_match_content_language_unavailable");
  validateAudienceLanguage(material, finding);
  return { ...evidence, reviewed_match: { verified: true, native_author_id: material.native_author_id, source_url: material.hit.source_url, review_id: reviewId, material_hash: material.material_hash, policy_version: INITIAL_MATCH_POLICY, outcome: finding.outcome, level: finding.match_level ?? null, reason: finding.rationale, language_only_pending: secondary !== undefined } };
}
