import { z } from "zod";
import type { DiscoveryCampaign, DiscoveryCandidate } from "./opencli-discovery";

const proof = z.object({ verified: z.literal(true), native_author_id: z.string(),
  review_id: z.string().min(1), source_url: z.string(), outcome: z.string().optional(),
  language: z.string().optional(), category: z.string().optional(), country_code: z.string().optional() });

function hasLanguageOnlyMatch(item: DiscoveryCandidate) {
  const match = (item.evidence.reviewed_selection ?? item.evidence.reviewed_match) as Record<string, unknown> | undefined;
  return match?.verified === true && ["initial-match-evaluation-v1", "selection-quality-evaluation-v2"].includes(String(match.policy_version))
    && match.native_author_id === item.platform_account_id && typeof match.review_id === "string" && match.review_id.length > 0
    && typeof match.source_url === "string" && item.evidence.source_urls.includes(match.source_url)
    && match.outcome === "insufficient" && match.language_only_pending === true;
}

export function candidateConditions(item: DiscoveryCandidate, criteria: DiscoveryCampaign["criteria"]) {
  const met: ("followersMet" | "pinterestMonthlyViewsMet" | "languageMet" | "categoryMet" | "regionMet")[] = [];
  const reasons = new Set(item.evidence.assessment?.reasons ?? []);
  const minimum = criteria.min_subscribers ?? criteria.min_followers;
  const count = item.evidence.assessment?.subscribers_normalized;
  const pinterestScale = item.platform === "pinterest" ? item.evidence.assessment?.pinterest_scale : undefined;
  if (pinterestScale?.state === "passed" && pinterestScale.passed_by?.includes("monthly_views")) met.push("pinterestMonthlyViewsMet");
  if (pinterestScale?.state === "passed" && pinterestScale.passed_by?.includes("followers")) met.push("followersMet");
  if (item.platform !== "pinterest" && minimum != null && count != null && count >= minimum
    && !reasons.has("subscriber_count_near_threshold") && !reasons.has("subscriber_count_unverified")) met.push("followersMet");
  for (const [criterion, key, label] of [
    ["language", "reviewed_language", "languageMet"],
    ["category", criteria.category === "服装" ? "reviewed_apparel_content" : "reviewed_category_content", "categoryMet"],
    ["region", "reviewed_residence", "regionMet"],
  ] as const) {
    const required = criteria[criterion]?.trim();
    if (!required || ["不限", "any", "all"].includes(required.toLowerCase())) continue;
    if (item.evidence.assessment?.selection?.state === "matched") {
      met.push(label);
      reasons.delete(`${criterion}_unverified`);
      continue;
    }
    if (criterion !== "language" && hasLanguageOnlyMatch(item) && !reasons.has(`${criterion}_mismatch`)) {
      met.push(label);
      reasons.delete(`${criterion}_unverified`);
      continue;
    }
    const parsed = proof.safeParse(item.evidence[key]);
    const bound = parsed.success && parsed.data.native_author_id === item.platform_account_id
      && item.evidence.source_urls.includes(parsed.data.source_url);
    const matches = bound && !reasons.has(`${criterion}_unverified`) && !reasons.has(`${criterion}_mismatch`) && (criterion === "language"
      ? parsed.data.language === required && parsed.data.outcome === "supported"
      : criterion === "category" ? required === "服装" || parsed.data.category === required
      : required === "中国大陆以外" && /^[A-Z]{2}$/.test(parsed.data.country_code ?? "") && parsed.data.country_code !== "CN");
    if (matches) met.push(label);
    else if (!(criterion === "language" && reasons.has("language_mismatch"))) reasons.add(`${criterion}_unverified`);
  }
  return { met, reasons: [...reasons] };
}

export function candidateRequirementGaps(item: DiscoveryCandidate, criteria: DiscoveryCampaign["criteria"]) {
  if (item.verdict === "qualified") return [];
  const requirement = z.object({ description: z.string(), excerpt: z.string() });
  const review = z.object({ index: z.number().int(), requirement, outcome: z.enum(["supported", "contradicted", "insufficient"]), reason: z.string(), native_author_id: z.string(), source_url: z.string() });
  const requirements = z.array(requirement).safeParse(criteria.requirements);
  if (!requirements.success) return [];
  const proofs = Array.isArray(item.evidence.reviewed_requirements) ? item.evidence.reviewed_requirements : [];
  return requirements.data.flatMap((condition, index) => {
    const proof = review.safeParse(proofs[index]);
    const bound = proof.success && proof.data.index === index && proof.data.native_author_id === item.platform_account_id
      && item.evidence.source_urls.includes(proof.data.source_url)
      && proof.data.requirement.description === condition.description && proof.data.requirement.excerpt === condition.excerpt;
    if (bound && proof.data.outcome === "supported") return [];
    return [{ condition: condition.excerpt, reason: bound ? proof.data.reason : "" }];
  });
}
