import { z } from "zod";
import { containsDiscoveryCitation, type DiscoveryReviewMaterial } from "./review-material.ts";
import type { DiscoveryReviewFinding } from "./initial-match.ts";

const requirements = z.array(z.strictObject({ description: z.string().min(1), excerpt: z.string().min(1) })).max(32);
export function applyDiscoveryRequirements(material: DiscoveryReviewMaterial, findings: readonly DiscoveryReviewFinding[], evidence: Record<string, unknown>, reviewId: string, revision: number): void {
  const criteria = z.record(z.string(), z.unknown()).parse(material.criteria);
  const expected = criteria.requirements === undefined ? [] : requirements.parse(criteria.requirements);
  const selected = findings.filter(finding => typeof finding.criterion === "object");
  if (selected.length !== expected.length) throw new Error("review_requirements_incomplete");
  evidence.reviewed_requirements = expected.map((requirement, index) => {
    const matching = selected.filter(finding => typeof finding.criterion === "object" && finding.criterion.requirement === index);
    if (matching.length !== 1) throw new Error("review_requirements_mismatch");
    const finding = matching[0]!;
    if (!finding.rationale.trim() || Buffer.byteLength(finding.rationale) > 4000 || finding.citations.length > 8 || finding.tool != null || finding.country_code != null || finding.match_level != null || !finding.citations.every(citation => containsDiscoveryCitation(material, citation)) || finding.outcome !== "insufficient" && finding.citations.length === 0) throw new Error("review_requirement_citation_invalid");
    return { requirement, index, outcome: finding.outcome, reason: finding.rationale, native_author_id: material.native_author_id, source_url: material.hit.source_url, review_id: reviewId, material_hash: material.material_hash, criteria_revision: revision };
  });
}
