import { z } from "zod";
import { discoveryIntentProposal, proposedDiscoveryConstraint, validateDiscoveryIntent } from "./intent.ts";
import type { DiscoveryIntentProposal } from "./intent.ts";
import { discoveryPlanInput, type DiscoveryPlanInput } from "./contracts.ts";
const text = z.string().refine(value => value.isWellFormed());
const boundedText = text.refine(value => Buffer.byteLength(value) <= 4000 && !/\p{Cc}/u.test(value));
const keyword = text.refine(value => !!value.trim() && Buffer.byteLength(value) <= 160 && !/\p{Cc}/u.test(value));
const count = z.number().int().min(1).max(10000);
export const prepareDiscoveryDraftInput = z.strictObject({
  id: text.regex(/^[A-Za-z\d_-]{1,128}$/u), brief: text.refine(value => !!value.trim() && Buffer.byteLength(value) <= 32000),
  profile_ref: text.refine(value => !!value.length && Buffer.byteLength(value) <= 128 && !/\p{Cc}/u.test(value)), opencli_version: z.literal("1.8.8"),
  target_count: count.nullish().transform(value => value ?? null), max_calls: count.nullish().transform(value => value ?? null), min_followers_override: z.number().int().nonnegative().safe().nullish().transform(value => value ?? null),
  read_budget: z.strictObject({ mode: z.enum(["default", "explicit"]), limit: count.nullable() }).optional(),
  criteria_overrides: z.strictObject({ region: boundedText.nullish().transform(value => value ?? null), language: boundedText.nullish().transform(value => value ?? null), category: boundedText.nullish().transform(value => value ?? null), requires_ai_tool_demo: z.boolean(), requires_public_tool_url: z.boolean() }).nullish().transform(value => value ?? null),
  scope_overrides: z.array(z.strictObject({ platform: text.refine(value => !!value.length && Buffer.byteLength(value) <= 128), keywords: z.array(keyword).min(1).max(20) })).max(64).default([]),
  refinement: z.strictObject({ campaign_id: text.min(1).max(128), expected_revision: z.number().int().positive().max(4294967295) }).nullish().transform(value => value ?? null),
}).refine(value => !(value.read_budget && value.max_calls !== null) && new Set(value.scope_overrides.map(scope => scope.platform)).size === value.scope_overrides.length && Buffer.byteLength(value.brief) <= (value.refinement === null ? 4000 : 32000));
const proposedCount = proposedDiscoveryConstraint(z.union([z.number().int().nonnegative().safe(), z.bigint().min(0n).max(18446744073709551615n)]));
export const semanticDiscoveryDraft = z.strictObject({ table_subject: text.default(""), target_count: proposedCount, max_calls: proposedCount, contact_search_terms: z.array(text).default([]), intent: discoveryIntentProposal, scopes: z.array(z.strictObject({ platform: text, keywords: z.array(text) })) });
export interface RegisteredDiscoverySource { readonly platform: string; readonly registered: boolean; readonly allowed_values: readonly string[]; readonly command?: string }
export class DiscoveryDraftError extends Error { readonly code: string; readonly excerpts: readonly string[]; constructor(code: string, excerpts: readonly string[] = []) { super(code); this.code = code; this.excerpts = excerpts; } }
export function isEnglishDiscoveryLanguage(value: string): boolean { return ["英文", "英语", "english", "en", "en-us", "en-gb"].includes(value.trim().toLowerCase()); }
export function discoveryQueryScript(value: string): RegExp | null {
  const language = value.trim().toLowerCase();
  if (["中文", "汉语", "chinese", "zh", "zh-cn", "zh-tw"].includes(language)) return /[\u3400-\u9fff]/u;
  if (["日语", "日文", "japanese", "ja"].includes(language)) return /[\u3040-\u30ff\u3400-\u9fff]/u;
  if (["韩语", "韩文", "korean", "ko"].includes(language)) return /[\uac00-\ud7af\u1100-\u11ff]/u;
  if (["俄语", "俄文", "russian", "ru"].includes(language)) return /[\u0400-\u052f]/u;
  if (["阿拉伯语", "arabic", "ar"].includes(language)) return /[\u0600-\u06ff]/u;
  return null;
}
export type DiscoveryDraftPrevious = Pick<DiscoveryPlanInput, "criteria" | "target_count" | "max_calls">;
export function buildDiscoveryDraft(raw: unknown, rawPlan: unknown, sources: readonly RegisteredDiscoverySource[], previous?: DiscoveryDraftPrevious): DiscoveryPlanInput {
  const input = prepareDiscoveryDraftInput.parse(raw), plan = semanticDiscoveryDraft.parse(rawPlan);
  const invalid = (code: string): never => { throw new DiscoveryDraftError(`discovery_draft_${code}`); };
  const quotedCount = (constraint: z.infer<typeof proposedCount>): number | null => {
    if (constraint.state === "unspecified") return null;
    if (!constraint.excerpt.trim() || !input.brief.includes(constraint.excerpt)) return invalid("intent_invalid");
    if (constraint.state === "unrestricted") return null;
    if (BigInt(constraint.value) < 1n || BigInt(constraint.value) > 10000n) return invalid("intent_invalid"); return Number(constraint.value);
  };
  if (plan.target_count.state === "unrestricted") {
    quotedCount(plan.target_count); throw new DiscoveryDraftError("discovery_draft_conditions_unsupported", [plan.target_count.excerpt]);
  }
  const target_count = input.target_count ?? quotedCount(plan.target_count) ?? previous?.target_count ?? 30;
  const inferredBudget = quotedCount(plan.max_calls);
  const max_calls = input.read_budget?.mode === "explicit" ? input.read_budget.limit
    : input.max_calls ?? (plan.max_calls.state === "unspecified" ? previous ? previous.max_calls : input.read_budget?.limit ?? null : inferredBudget);
  plan.intent.region = { state: "unspecified" };
  if (input.criteria_overrides) for (const [label, field] of [["Content language", "language"], ["Content category", "category"]] as const) {
    const value = input.criteria_overrides[field]; if (value === null) continue; const line = `${label}: ${value}`;
    if (input.brief.split("\n").includes(line)) { plan.intent[field] = { state: "specified", value, excerpt: line }; plan.intent.other_hard_conditions = plan.intent.other_hard_conditions.filter(condition => condition.excerpt !== line && condition.excerpt !== value); }
  }
  const registered = sources.filter(source => source.registered), seenSuggested = new Set<string>();
  plan.intent.suggested_keywords = plan.intent.suggested_keywords.filter(value => { const key = value.trim().toLowerCase(); if (seenSuggested.has(key)) return false; seenSuggested.add(key); return true; });
  const issues = validateDiscoveryIntent(input.brief, plan.intent, registered.map(source => source.platform));
  if (issues.length) invalid("intent_invalid");
  if (plan.scopes.length > registered.length) invalid("scope_invalid");
  const textValue = (value: DiscoveryIntentProposal["category"]) => value.state === "specified" ? value.value : "";
  const inheritedText = (key: "language" | "category") => plan.intent[key].state === "unspecified" && typeof previous?.criteria[key] === "string" ? previous.criteria[key] : textValue(plan.intent[key]);
  const language = input.criteria_overrides?.language ?? inheritedText("language");
  for (const override of input.scope_overrides) {
    const source = registered.find(source => source.platform === override.platform); if (!source) invalid("plan_invalid");
    const translate = !!language.trim() && !/-(?:tag|node|subreddit)$/u.test(override.platform) && source!.allowed_values.length === 0;
    const existing = plan.scopes.find(scope => scope.platform === override.platform);
    if (existing) {
      if (translate) { if (existing.keywords.length !== override.keywords.length) invalid("scope_invalid"); const seen = new Set<string>(); existing.keywords = existing.keywords.filter(value => { const key = value.trim().toLowerCase(); if (seen.has(key)) return false; seen.add(key); return true; }); }
      else existing.keywords = [...override.keywords];
    } else if (translate) invalid("scope_invalid"); else plan.scopes.push({ platform: override.platform, keywords: [...override.keywords] });
  }
  const seen = new Set<string>(), nonLatin = /[\u3400-\u9fff\u3040-\u30ff\uac00-\ud7af\u1100-\u11ff\u0400-\u052f\u0600-\u06ff]/u;
  for (const scope of plan.scopes) {
    const source = registered.find(source => source.platform === scope.platform); if (!source) invalid("plan_invalid");
    const named = /-(?:tag|node|subreddit)$/u.test(scope.platform) || source!.allowed_values.length > 0;
    if (!named) {
      if (isEnglishDiscoveryLanguage(language) && scope.keywords.some(value => !/[A-Za-z]/u.test(value) || nonLatin.test(value))) invalid("scope_invalid");
      const script = discoveryQueryScript(language);
      if (script) { const manual = input.scope_overrides.some(value => value.platform === scope.platform) && !!language.trim(); if (!scope.keywords.some(value => script.test(value)) || manual && scope.keywords.some(value => !script.test(value))) invalid("scope_invalid"); scope.keywords.sort((a, b) => Number(!script.test(a)) - Number(!script.test(b))); }
    }
    if (seen.has(scope.platform) || !scope.keywords.length || scope.keywords.length > 20 || scope.keywords.some(value => !keyword.safeParse(value).success) || source!.allowed_values.length > 0 && scope.keywords.some(value => !source!.allowed_values.includes(value)) || plan.intent.excluded_platforms.some(value => value.platform === scope.platform)) invalid("scope_invalid");
    seen.add(scope.platform);
  }
  if (registered.some(source => !seen.has(source.platform) && !plan.intent.excluded_platforms.some(value => value.platform === source.platform))) invalid("coverage_incomplete");
  if (plan.contact_search_terms.length > 6 || plan.contact_search_terms.some(term => !term.trim() || [...term].length > 50 || /["():|<>\\/@\p{Cc}]/u.test(term))) invalid("plan_invalid");
  const min = plan.intent.min_subscribers, threshold = input.min_followers_override ?? (min.state === "specified" ? min.value : min.state === "unrestricted" ? 0 : previous ? previous.criteria.min_subscribers ?? previous.criteria.min_followers ?? 0 : 2000);
  const criteria: Record<string, unknown> = { category: inheritedText("category"), region: "", language: inheritedText("language"), min_subscribers: threshold, requires_ai_tool_demo: false, requires_public_tool_url: false };
  if (plan.intent.other_hard_conditions.length) criteria.requirements = plan.intent.other_hard_conditions;
  criteria.contact_preference = "public_email_first";
  if (input.refinement !== null) criteria.refinement = input.refinement;
  if (input.criteria_overrides) { for (const key of ["language", "category"] as const) if (input.criteria_overrides[key] !== null) criteria[key] = input.criteria_overrides[key]; criteria.requires_ai_tool_demo = input.criteria_overrides.requires_ai_tool_demo; criteria.requires_public_tool_url = input.criteria_overrides.requires_public_tool_url; }
  if (plan.contact_search_terms.length) criteria.contact_search_terms = plan.contact_search_terms;
  criteria.automatic_query_expansion = input.scope_overrides.length === 0;
  if (plan.table_subject.trim()) criteria.table_subject = [...plan.table_subject.trim()].filter(character => !/\p{Cc}/u.test(character)).slice(0, 40).join("");
  return discoveryPlanInput.parse({ id: input.id, brief: input.brief, opencli_version: input.opencli_version, criteria, scopes: plan.scopes.map(scope => ({ ...scope, profile_ref: input.profile_ref })), target_count, max_calls });
}
