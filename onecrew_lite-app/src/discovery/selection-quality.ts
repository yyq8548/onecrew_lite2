import { discoveryQualityScore } from "@onecrew/runtime-contracts/discovery";
import { containsDiscoveryCitation, type DiscoveryReviewMaterial } from "./review-material.ts";
import { discoveryScale } from "./qualification.ts";
import type { DiscoveryReviewFinding } from "./initial-match.ts";
import { validateAudienceLanguage } from "./initial-match.ts";
import { SELECTION_QUALITY_POLICY } from "./quality-policy.ts";

const object = (value: unknown): Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
export function applyDiscoverySelectionQuality(material: DiscoveryReviewMaterial, findings: readonly DiscoveryReviewFinding[], raw: unknown, reviewId: string): Record<string, unknown> {
  const selections = findings.filter(item => item.criterion === "selection_fit"), qualities = findings.filter(item => item.criterion === "quality_rating");
  if (selections.length !== 1 || qualities.length !== 1 || findings.some(item => typeof item.criterion === "string" && !["selection_fit", "quality_rating", "content_language"].includes(item.criterion))) throw new Error("selection_quality_findings_required");
  const selection = selections[0]!, quality = qualities[0]!;
  for (const finding of [selection, quality]) {
    if (!finding.rationale.trim() || Buffer.byteLength(finding.rationale) > 4000 || finding.tool != null || finding.country_code != null || finding.citations.length > 8 || !finding.citations.every(citation => containsDiscoveryCitation(material, citation))) throw new Error("selection_quality_citation_invalid");
  }
  if (selection.match_level != null || quality.outcome === "contradicted" || (quality.outcome === "supported") !== (quality.match_level != null)) throw new Error("selection_quality_level_invalid");
  const estimate = material.native_follower_estimate, savedCount = object(object(raw).assessment).subscribers_normalized, count = material.native_followers ?? (typeof savedCount === "number" || typeof savedCount === "bigint" ? savedCount : null);
  const scale = discoveryScale(material.criteria, estimate ? { nominal: BigInt(estimate.nominal), lower: BigInt(estimate.lower), upper: BigInt(estimate.upper) } : count == null ? null : { nominal: BigInt(count), lower: BigInt(count), upper: BigInt(count) });
  if (scale.state === "failed" && selection.outcome !== "contradicted" || scale.state === "unknown" && selection.outcome === "supported") throw new Error("selection_quality_required_count_invalid");
  const authorCitation = (finding: DiscoveryReviewFinding) => finding.citations.some(citation => ["description", "search_caption", "transcript", "native_content", "native_profile"].includes(citation.kind));
  if (selection.outcome !== "insufficient" && !(scale.state === "failed" && selection.outcome === "contradicted") && !authorCitation(selection) || quality.outcome === "supported" && !authorCitation(quality)) throw new Error("selection_quality_author_evidence_required");
  const criteria = object(material.criteria), language = typeof criteria.language === "string" ? criteria.language.trim() : "";
  const audience = selection.rationale.includes("粉丝评论语言符合") || selection.rationale.includes("Audience comment language matches");
  if (selection.outcome === "supported" && language && selection.citations.some(citation => citation.kind === "native_profile") && !selection.citations.some(citation => ["description", "search_caption", "transcript", "native_content"].includes(citation.kind))) throw new Error("initial_match_content_language_unavailable");
  if (audience && /spoken|speech|口语|口播|语音/iu.test(language)) throw new Error("initial_match_audience_language_invalid");
  validateAudienceLanguage(material, selection, true);
  const pending = findings.filter(item => item.criterion === "content_language");
  if (pending.length > 1 || pending.some(item => selection.outcome !== "insufficient" || item.outcome !== "insufficient" || item.match_level != null || item.tool != null || item.country_code != null || !language || !item.rationale.trim() || Buffer.byteLength(item.rationale) > 4000 || item.citations.length > 8 || !item.citations.every(citation => containsDiscoveryCitation(material, citation)))) throw new Error("initial_match_language_request_invalid");
  const proof = { verified: true, native_author_id: material.native_author_id, source_url: material.hit.source_url, review_id: reviewId, material_hash: material.material_hash, policy_version: SELECTION_QUALITY_POLICY };
  return { ...object(raw), reviewed_selection: { ...proof, outcome: selection.outcome, reason: selection.rationale, language_only_pending: pending.length === 1 }, reviewed_quality: { ...proof, outcome: quality.outcome, level: quality.match_level ?? null, score: quality.match_level == null ? null : discoveryQualityScore(quality.match_level), reason: quality.rationale } };
}
