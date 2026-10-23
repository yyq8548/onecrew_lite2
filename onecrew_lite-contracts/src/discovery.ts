import { z } from "zod";
import { creatorU64 } from "./creators.ts";

export const DISCOVERY_QUALITY_SCORES = Object.freeze({ excellent: 95, good: 85, medium: 70, low: 40 } as const);
export type DiscoveryQualityLevel = keyof typeof DISCOVERY_QUALITY_SCORES;
export function discoveryQualityScore(level: DiscoveryQualityLevel): 95 | 85 | 70 | 40 { return DISCOVERY_QUALITY_SCORES[level]; }
export const discoveryQualityMinimum = z.strictObject({ kind: z.literal("minimum"), operator: z.enum(["gte", "gt"]), score: z.number().int().min(0).max(100), excerpt: z.string().min(1).max(1000) });
export type DiscoveryQualityFilter = z.infer<typeof discoveryQualityMinimum> | { kind: "requirement"; index: number } | { kind: "all" | "any"; items: DiscoveryQualityFilter[] };
export const discoveryQualityFilter: z.ZodType<DiscoveryQualityFilter> = z.lazy(() => z.union([
  discoveryQualityMinimum, z.strictObject({ kind: z.literal("requirement"), index: z.number().int().min(0).max(31) }),
  z.strictObject({ kind: z.enum(["all", "any"]), items: z.array(discoveryQualityFilter).min(1).max(32) }),
]));

const text = z.string();
const strings = z.array(text);
const signedTimestamp = z.number().int().safe();
const u32 = z.number().int().min(0).max(4_294_967_295);
const nullableText = text.nullable();
const observation = z.object({ source_url: text, text }).strict();
const languageSample = z.object({ author_id: text, source_url: text, text, comments: z.array(observation) }).strict();
export const discoveryHit = z.object({ id: text, campaign_id: text, platform: text, profile_ref: text, search_keyword: text, source_url: text, title: text, author_label: text, captured_at: signedTimestamp, author_id: nullableText, author_url: nullableText, author_observed_at: signedTimestamp.nullable() }).passthrough();
const claim = z.object({ campaign_id: text, expected_revision: u32, search_hit_id: text, platform: text, profile_ref: text, author_id: text, source_url: text, call_number: u32 }).passthrough();
const capture = { candidate_id: text, search_hit_id: text, profile_ref: text, platform_account_id: text, call_number: u32, status: text, stop_code: nullableText, captured_at: signedTimestamp };
const transcript = z.object({ ...capture, source_url: text, segments: z.array(z.object({ timestamp: text, speaker: text, text }).passthrough()), truncated: z.boolean() }).passthrough();
const publicPage = z.object({ ...capture, requested_url: text, source_url: text, source_excerpt: text, document_url: nullableText, title: text, markdown_excerpt: text, truncated: z.boolean() }).passthrough();
export const discoveryReviewMaterial = z.object({
  format_version: u32, campaign_id: text, criteria_revision: u32, criteria: z.unknown(),
  approved_scopes: z.array(z.object({ platform: text, profile_ref: text, keywords: strings }).passthrough()), approved_at: signedTimestamp,
  hit: discoveryHit, candidate_id: text, native_author_id: text, candidate_evidence_hash: text,
  video_description: nullableText, transcript: transcript.nullable(), public_pages: z.array(publicPage), material_hash: text,
  language_samples: z.array(languageSample).optional(),
  native_content: z.object({ claim, status: text, native_content: z.unknown(), stop_code: nullableText, captured_at: signedTimestamp, language_sample: languageSample.nullish() }).passthrough().nullish(),
  native_follower_estimate: z.object({ display: text, lower: creatorU64, nominal: creatorU64, upper: creatorU64 }).passthrough().nullish(),
  native_followers: creatorU64.nullish(), native_platform: nullableText.optional(),
  native_profile: z.object({ profile_url: text, observed_at: signedTimestamp, bio: nullableText.optional(), self_declared_location: nullableText.optional(), web_text: nullableText.optional(), bio_status: z.enum(["complete", "truncated", "missing", "unknown", "unavailable"]).optional() }).passthrough().nullish(),
}).passthrough().refine(value => Object.hasOwn(value, "criteria"));
export const discoveryReviewCriterion = z.union([z.enum(["own_ai_tool_demo", "public_tool_url", "apparel_business_content", "category_content", "creator_residence_outside_mainland", "content_language", "match_fit", "selection_fit", "quality_rating"]), z.object({ requirement: z.number().int().min(0).max(31) }).strict()]);
export type DiscoveryReviewCriterion = z.infer<typeof discoveryReviewCriterion>;
export const discoveryReviewReceipt = z.object({
  review_id: text, operator_account_id: text, campaign_id: text, criteria_revision: u32, search_hit_id: text,
  candidate_id: text, native_author_id: text, profile_ref: text, request_id: text, material_hash: text, policy_version: text,
  judgments: z.array(z.object({ criterion: discoveryReviewCriterion, outcome: z.enum(["supported", "contradicted", "insufficient"]) }).passthrough()),
  verdict: text, reasons: strings, subscribers: creatorU64.nullable(), created_at: signedTimestamp,
}).passthrough();
export type DiscoveryReviewMaterial = z.infer<typeof discoveryReviewMaterial>;
export type DiscoveryReviewReceipt = z.infer<typeof discoveryReviewReceipt>;
export const discoveryIdentityMergeMaterial = z.object({ format_version: u32, campaign_id: text, criteria_revision: u32, accounts: z.array(discoveryReviewMaterial).length(2), material_hash: text }).passthrough();
export type DiscoveryIdentityMergeMaterial = z.infer<typeof discoveryIdentityMergeMaterial>;
const draftPhase = z.enum(["checking_connection", "checking_codex", "planning", "validating_plan", "saving_plan", "finished", "failed"]);
export const discoveryDraftProgress = z.object({ id: text, phase: draftPhase, elapsed_ms: creatorU64, phase_elapsed_ms: creatorU64, timings: z.array(z.object({ phase: draftPhase, elapsed_ms: creatorU64 }).strict()), error_code: nullableText, unsupported_conditions: strings }).strict();
export const discoveryExecutionStatus = z.object({
  pipeline: z.object({ active_pending: u32, queued_scores: u32, running_scores: u32, discovery_held: z.boolean(), contact_pending: z.boolean() }).strict().optional(),
  campaign_id: text, revision: u32, stage: z.enum(["ready", "searching", "verifying", "pausing", "paused", "finished", "needs_evidence", "error"]), used: u32,
  deferred_hits: z.array(z.object({ reason: text, search_hit_id: text }).strict()), deferred_scores: u32, excluded_platforms: strings,
  failure_code: nullableText, failure_phase: nullableText, reason: nullableText, sync_error: nullableText, sync_pending: z.boolean(), synced_at: creatorU64.nullable(), updated_at: creatorU64,
  library: z.object({ campaign_id: text, table_id: text, web_url: text, total_hits: creatorU64, inserted_rows: creatorU64 }).strict().nullable(),
}).strict();
export const componentsReport = z.object({ binary_sha256: text, built_at: creatorU64, checked_at: creatorU64, library_state: text, library_version: nullableText, opencli_state: text, opencli_version: nullableText, plugin_state: text, plugin_version: nullableText, version: text }).strict();
export type DraftProgress = z.infer<typeof discoveryDraftProgress>;
export type ExecutionStatus = z.infer<typeof discoveryExecutionStatus>;
