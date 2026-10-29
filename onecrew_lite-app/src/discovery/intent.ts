import { z } from "zod";
import { proposedQualityFilter, validateProposedQualityFilter } from "./quality-policy.ts";
const text = z.string().refine(value => value.isWellFormed());
export const DISCOVERY_GOAL_MAX_CHARACTERS = 400;
export function proposedDiscoveryConstraint<T extends z.ZodType>(value: T) { return z.discriminatedUnion("state", [z.strictObject({ state: z.literal("specified"), value, excerpt: text }), z.strictObject({ state: z.literal("unrestricted"), excerpt: text }), z.strictObject({ state: z.literal("unspecified") })]); }
export const discoveryIntentProposal = z.strictObject({
  original_request: text, goal: text, category: proposedDiscoveryConstraint(text), region: proposedDiscoveryConstraint(text), language: proposedDiscoveryConstraint(text),
  min_subscribers: proposedDiscoveryConstraint(z.union([z.number().int().safe().nonnegative(), z.bigint().min(0n).max(18446744073709551615n)])),
  quality_filter: proposedDiscoveryConstraint(proposedQualityFilter).optional(),
  occupational_review_applicability: z.enum(["required", "not_required", "unknown"]).default("unknown").describe("Assess the complete objective, category, every hard condition and all quality-filter branches. Use required for a requested employer, occupational role or employment relationship; not_required only when none is required; unknown when applicability is unclear."),
  excluded_platforms: z.array(z.strictObject({ platform: text, excerpt: text })),
  other_hard_conditions: z.array(z.strictObject({ description: text, excerpt: text })),
  soft_preferences: z.array(z.strictObject({ description: text, excerpt: text.nullable() })), suggested_keywords: z.array(text),
});
export type DiscoveryIntentProposal = z.infer<typeof discoveryIntentProposal>;
export interface DiscoveryIntentIssue { field: string; code: string }
export function numbersInDiscoveryExcerpt(excerpt: string): bigint[] {
  const values: bigint[] = []; let digits = "";
  for (const character of `${excerpt} `) {
    if (/^[0-9]$/u.test(character)) digits += character;
    else if (character === "," && digits.length) continue;
    else if (digits.length) {
      const value = BigInt(digits), multiplier = character === "万" ? 10000n : ["千", "k", "K"].includes(character) ? 1000n : 1n;
      if (value <= 18446744073709551615n && value * multiplier <= 18446744073709551615n) values.push(value * multiplier);
      digits = "";
    }
  }
  return values;
}
export function validateDiscoveryIntent(request: string, raw: unknown, registeredPlatforms: readonly string[]): DiscoveryIntentIssue[] {
  const proposal = discoveryIntentProposal.parse(raw), issues: DiscoveryIntentIssue[] = [];
  const issue = (field: string, code: string) => issues.push({ field, code });
  const excerptValid = (value: string) => !!value.trim() && request.includes(value);
  const bytes = (value: string) => Buffer.byteLength(value);
  if (!request.trim() || bytes(request) > 32000 || proposal.original_request !== request) issue("original_request", "request_mismatch");
  if (proposal.excluded_platforms.length > 64) issue("excluded_platforms", "too_many_values");
  if (proposal.other_hard_conditions.length > 32) issue("other_hard_conditions", "too_many_values");
  if (proposal.soft_preferences.length > 64) issue("soft_preferences", "too_many_values");
  if (!proposal.goal.trim() || [...proposal.goal].length > DISCOVERY_GOAL_MAX_CHARACTERS) issue("goal", "invalid_value");
  for (const field of ["category", "region", "language"] as const) {
    const value = proposal[field];
    if (value.state === "specified" && (!value.value.trim() || bytes(value.value) > 160)) issue(field, "invalid_value");
    if (value.state !== "unspecified" && !excerptValid(value.excerpt)) issue(field, "excerpt_not_in_request");
  }
  const count = proposal.min_subscribers;
  const quality = proposal.quality_filter;
  if (quality && quality.state !== "unspecified") {
    if (!excerptValid(quality.excerpt)) issue("quality_filter", "excerpt_not_in_request");
    if (quality.state === "specified" && !validateProposedQualityFilter(request, quality.value)) issue("quality_filter", "invalid_quality_requirement");
  }
  if (count.state === "specified") {
    if (BigInt(count.value) > 1000000000n) issue("min_subscribers", "invalid_value");
    if (!excerptValid(count.excerpt)) issue("min_subscribers", "excerpt_not_in_request");
    else if (!numbersInDiscoveryExcerpt(count.excerpt).includes(BigInt(count.value))) issue("min_subscribers", "number_not_in_excerpt");
  } else if (count.state === "unrestricted" && !excerptValid(count.excerpt)) issue("min_subscribers", "excerpt_not_in_request");
  const excluded = new Set<string>();
  for (const platform of proposal.excluded_platforms) {
    if (!registeredPlatforms.includes(platform.platform) || excluded.has(platform.platform)) issue("excluded_platforms", "invalid_platform");
    excluded.add(platform.platform); if (!excerptValid(platform.excerpt)) issue("excluded_platforms", "excerpt_not_in_request");
  }
  for (const condition of proposal.other_hard_conditions) if (!excerptValid(condition.excerpt) || !condition.description.trim() || bytes(condition.description) > 1000) issue("other_hard_conditions", "invalid_condition");
  for (const preference of proposal.soft_preferences) {
    if (!preference.description.trim() || bytes(preference.description) > 300) issue("soft_preferences", "invalid_value");
    if (preference.excerpt !== null && !excerptValid(preference.excerpt)) issue("soft_preferences", "excerpt_not_in_request");
  }
  if (proposal.suggested_keywords.length > 20) issue("suggested_keywords", "invalid_count");
  const seen = new Set<string>();
  for (const keyword of proposal.suggested_keywords) {
    const normalized = keyword.trim().toLowerCase();
    if (!normalized || bytes(keyword) > 160 || /\p{Cc}/u.test(keyword) || seen.has(normalized)) issue("suggested_keywords", "invalid_keyword");
    seen.add(normalized);
  }
  return issues;
}
