import { z } from "zod";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import { CREATOR_SEARCH_SOURCES } from "../platforms/opencli/sources.ts";

const text = z.string().refine(value => value.isWellFormed());
const chars = (value: string) => [...value].length;
const control = (value: string) => /\p{Cc}/u.test(value);
export const discoveryScope = z.strictObject({
  platform: text.regex(/^[a-z\d-]{1,64}$/u),
  profile_ref: text.regex(/^[A-Za-z\d_.-]{1,64}$/u),
  keywords: z.array(text.refine(value => !!value.trim() && chars(value) <= 160 && !control(value))).min(1).max(20),
});
export const discoveryCriteria = z.record(z.string(), z.unknown()).superRefine((value, context) => {
  const fail = (path: string, message: string) => context.addIssue({ code: "custom", path: [path], message });
  if (Buffer.byteLength(stringifyApplicationJson(value)) > 16384) fail("", "criteria_too_large");
  if (Object.hasOwn(value, "min_subscribers") && Object.hasOwn(value, "min_followers")) fail("min_followers", "competing_follower_thresholds");
  for (const key of ["min_subscribers", "min_followers"]) if (Object.hasOwn(value, key) && !z.union([z.number().int().safe().nonnegative(), z.bigint().min(0n).max(18446744073709551615n)]).safeParse(value[key]).success) fail(key, "invalid_follower_threshold");
  if (Object.hasOwn(value, "pinterest_min_monthly_views_exclusive") && !z.number().int().safe().nonnegative().safeParse(value.pinterest_min_monthly_views_exclusive).success) fail("pinterest_min_monthly_views_exclusive", "invalid_monthly_views_threshold");
  for (const key of ["requires_ai_tool_demo", "requires_public_tool_url"]) if (Object.hasOwn(value, key) && typeof value[key] !== "boolean") fail(key, "invalid_evidence_requirement");
  for (const key of ["region", "language"]) if (Object.hasOwn(value, key) && (typeof value[key] !== "string" || !value[key].isWellFormed() || chars(value[key]) > 160 || control(value[key]))) fail(key, "invalid_criteria_text");
  if (value.contact_preference !== undefined && value.contact_preference !== "public_email_first") fail("contact_preference", "invalid_contact_preference");
  if (value.requirements !== undefined && !z.array(z.strictObject({ description: text.refine(value => !!value.trim() && Buffer.byteLength(value) <= 1000), excerpt: text.refine(value => !!value.trim()) })).max(32).safeParse(value.requirements).success) fail("requirements", "invalid_requirements");
  if (value.excluded_accounts !== undefined && !z.array(z.strictObject({ native_author_id: text.min(1).max(256), profile_url: z.url().startsWith("https://").max(2048), excerpt: text.min(1) })).max(1000).safeParse(value.excluded_accounts).success) fail("excluded_accounts", "invalid_excluded_accounts");
  if (value.refinement !== undefined && !z.strictObject({ campaign_id: text.min(1).max(128), expected_revision: z.number().int().positive().max(4294967295) }).safeParse(value.refinement).success) fail("refinement", "invalid_refinement");
});
export const discoveryPlanInput = z.strictObject({
  id: text.regex(/^[A-Za-z\d_-]{1,128}$/u),
  brief: text.refine(value => !!value.trim() && [...value].length <= 32000),
  opencli_version: text.regex(/^\d+\.\d+\.\d+$/u).refine(value => value.length <= 64),
  criteria: discoveryCriteria,
  scopes: z.array(discoveryScope).min(1).max(64),
  target_count: z.number().int().min(1).max(4294967295),
  max_calls: z.number().int().min(1).max(10000).nullable(),
}).refine(value => new Set(value.scopes.map(scope => `${scope.platform}\0${scope.profile_ref}`)).size === value.scopes.length, "duplicate_platform_profile_scope")
  .refine(value => [...value.brief].length <= (value.criteria.refinement === undefined ? 4000 : 32000), "brief_too_large")
  .superRefine((value, context) => {
    for (const key of ["requirements", "excluded_accounts"]) {
      const items = value.criteria[key];
      if (Array.isArray(items) && items.some(item => !value.brief.includes((item as { excerpt: string }).excerpt))) context.addIssue({ code: "custom", path: ["criteria", key], message: "customer_quotation_required" });
    }
    const source = value.criteria.refinement as { campaign_id: string } | undefined;
    if (source?.campaign_id === value.id) context.addIssue({ code: "custom", path: ["criteria", "refinement"], message: "refinement_source_conflict" });
  });
export type DiscoveryScope = z.infer<typeof discoveryScope>;
export type DiscoveryPlanInput = z.infer<typeof discoveryPlanInput>;
export function assertRegisteredDiscoveryScopes(scopes: readonly DiscoveryScope[]): void {
  for (const scope of scopes) if (!Object.hasOwn(CREATOR_SEARCH_SOURCES, scope.platform) || !CREATOR_SEARCH_SOURCES[scope.platform]!.discovery) throw new Error("discovery_source_unavailable");
}
export function appendDiscoveryQueries(expected: readonly DiscoveryScope[], additions: readonly DiscoveryScope[]): DiscoveryScope[] {
  if (!additions.length || additions.length > expected.length) throw new Error("query_round_invalid");
  const platforms = new Set<string>();
  for (const scope of additions) {
    const key = `${scope.platform}\0${scope.profile_ref}`;
    if (platforms.has(key) || !scope.keywords.length || scope.keywords.length > 8) throw new Error("query_round_invalid");
    platforms.add(key);
    const prior = expected.find(value => value.platform === scope.platform && value.profile_ref === scope.profile_ref);
    if (!prior) throw new Error("query_round_outside_scope");
    const seen = new Set(prior.keywords.map(value => value.trim().toLowerCase()));
    for (const word of scope.keywords) {
      const normalized = word.trim().toLowerCase();
      if (!word.isWellFormed() || !normalized || Buffer.byteLength(word) > 160 || control(word) || seen.has(normalized)) throw new Error("query_round_duplicate_or_invalid");
      seen.add(normalized);
    }
  }
  return expected.map(scope => ({ ...scope, keywords: [...scope.keywords, ...additions.filter(addition => addition.platform === scope.platform && addition.profile_ref === scope.profile_ref).flatMap(addition => addition.keywords)] }));
}
