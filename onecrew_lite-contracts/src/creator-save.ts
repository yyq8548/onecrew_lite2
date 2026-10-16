import { stringify } from "lossless-json";
import { z } from "zod";
import { platform } from "./creators.ts";
import { resultByteLength } from "./fields.ts";
import { isWellFormedUnicode } from "./json.ts";

const identifier = (maximum = 128) => z.string().regex(/^[A-Za-z0-9._:-]+$/).refine(value => resultByteLength(value) <= maximum);
const text = z.string().refine(isWellFormedUnicode);
const count = z.number().int().nonnegative().safe();
export const resultU64 = z.union([z.bigint().min(0n).max(18446744073709551615n), z.number().int().nonnegative().safe().transform(BigInt)]);
export const creatorSaveIntent = z.enum(["add_new", "save_or_update"]);
export const creatorSavePreparedStatus = z.enum(["eligible_new", "eligible_update", "existing_skipped", "duplicate_skipped", "identity_pending", "contact_conflict", "failed"]);
export const creatorSavePreparedReason = z.enum(["existing_creator", "duplicate_selection", "identity_ambiguous", "conflicting_update", "required_contact_route_conflict", "preparation_failed"]);
const collision = z.object({ status: z.enum(["none", "possible", "conflict"]), creator_ids: z.array(text) }).strict();
export const creatorSaveRoute = z.object({ route_id: identifier(), channel: text, kind: text, display_value: text, normalized_value: text, sources: z.array(text), required: z.boolean(), collision }).strict();
export const creatorSavePreview = z.object({
  display_name: text.nullable(), platform, handle: text.nullable(), profile_url: text.nullable(), source_url: text.nullable(), avatar_url: text.nullable().default(null), identity_status: text, identity_creator_ids: z.array(text),
  intent: creatorSaveIntent, status: creatorSavePreparedStatus, reason: creatorSavePreparedReason.nullable(), notes: text.nullable(), score: z.number().int().min(0).max(255).nullable(), assessment: text.nullable(),
  follower_counts: z.record(text, resultU64), routes: z.array(creatorSaveRoute).min(1).max(128), expires_at: resultU64,
}).strict().superRefine((value, context) => {
  if (resultByteLength(stringify(value)!) > 64 * 1024 || !value.routes.some(route => route.required) || new Set(value.routes.map(route => route.route_id)).size !== value.routes.length) context.addIssue({ code: "custom", message: "creator_save_preview" });
  if (value.intent === "add_new" && value.status === "eligible_update" || value.intent === "save_or_update" && value.status === "existing_skipped") context.addIssue({ code: "custom", message: "creator_save_preview_intent" });
  const reasons = { eligible_new: [null], eligible_update: [null], existing_skipped: ["existing_creator"], duplicate_skipped: ["duplicate_selection"], identity_pending: ["identity_ambiguous", "conflicting_update"], contact_conflict: ["required_contact_route_conflict"], failed: ["preparation_failed"] } as const;
  if (!(reasons[value.status] as readonly (string | null)[]).includes(value.reason)) context.addIssue({ code: "custom", message: "creator_save_preview_reason" });
});
export const creatorSavePreparedSummary = z.object({ total: count, eligible_new: count, eligible_update: count, existing_skipped: count, duplicate_skipped: count, blocked: count, failed: count }).strict();
const boundedOptionalText = text.refine(value => resultByteLength(value) <= 8192).nullable();
export const creatorSavePreparedFailure = z.object({ row_id: identifier(), display_name: boundedOptionalText, platform, handle: boundedOptionalText, profile_url: boundedOptionalText, reason: creatorSavePreparedReason }).strict();
export const creatorSaveReview = z.object({ rows: z.array(z.object({ row_id: identifier(), preview: creatorSavePreview }).strict()).max(50), failures: z.array(creatorSavePreparedFailure).max(50), intent: creatorSaveIntent, summary: creatorSavePreparedSummary }).strict().superRefine((value, context) => {
  const ids = [...value.rows.map(row => row.row_id), ...value.failures.map(row => row.row_id)];
  if (ids.length === 0 || ids.length > 50 || new Set(ids).size !== ids.length) context.addIssue({ code: "custom", message: "creator_save_duplicate_row" });
  if (value.rows.some(row => row.preview.intent !== value.intent)) context.addIssue({ code: "custom", message: "creator_save_review_intent" });
  const expected = { total: ids.length, eligible_new: 0, eligible_update: 0, existing_skipped: 0, duplicate_skipped: 0, blocked: 0, failed: value.failures.length };
  for (const row of value.rows) { const key = row.preview.status === "identity_pending" || row.preview.status === "contact_conflict" ? "blocked" : row.preview.status; expected[key]++; }
  if (Object.keys(expected).some(key => expected[key as keyof typeof expected] !== value.summary[key as keyof typeof expected])) context.addIssue({ code: "custom", message: "creator_save_review_summary" });
});
export const creatorSaveSelection = z.object({ row_id: identifier(), confirmed_route_ids: z.array(identifier()).min(1).max(128) }).strict();
export const creatorSaveRowStatus = z.enum(["saved", "failed", "unknown", "not_run", "not_selected"]);
export const creatorSaveOperation = z.enum(["added", "updated", "existing_skipped", "duplicate_skipped", "not_selected", "failed", "unknown"]);
export const creatorSaveReason = z.enum(["existing_creator", "duplicate_selection", "identity_ambiguous", "required_contact_route_conflict", "conflicting_update", "preparation_failed", "user_not_selected", "row_failed", "commit_unknown"]);
const outcomeFields = { row_id: identifier(), status: creatorSaveRowStatus, operation: creatorSaveOperation, reason: creatorSaveReason.nullable(), creator_id: identifier(256).nullable(), failure_code: identifier().nullable() };
function outcomeValid(value: { status: string; operation: string; reason: string | null; creator_id: string | null }): boolean {
  if ((value.status === "saved") !== (value.creator_id !== null)) return false;
  switch (value.operation) {
    case "added": case "updated": return value.status === "saved" && value.reason === null;
    case "existing_skipped": case "duplicate_skipped": return ["not_run", "not_selected"].includes(value.status) && value.reason === (value.operation === "existing_skipped" ? "existing_creator" : "duplicate_selection");
    case "not_selected": return value.status === "not_selected" && value.reason === "user_not_selected";
    case "failed": return ["failed", "not_run"].includes(value.status) && ["identity_ambiguous", "required_contact_route_conflict", "conflicting_update", "preparation_failed", "row_failed"].includes(value.reason ?? "");
    case "unknown": return value.status === "unknown" && value.reason === "commit_unknown";
    default: return false;
  }
}
export const creatorSaveOutcome = z.object({ ...outcomeFields, display_name: text.nullable(), platform, handle: text.nullable(), profile_url: text.nullable(), avatar_url: text.nullable(), confirmed_route_ids: z.array(identifier()).max(128) }).strict().refine(outcomeValid, "creator_save_outcome_operation");
export const creatorSaveOutcomes = z.array(creatorSaveOutcome).min(1).max(50).refine(value => new Set(value.map(row => row.row_id)).size === value.length, "creator_save_duplicate_row");
export const creatorSaveContinuationOutcome = z.object(outcomeFields).strict().refine(outcomeValid, "creator_save_outcome_operation");
export const creatorSaveResultSummary = z.object({ total: count, added: count, updated: count, existing_skipped: count, duplicate_skipped: count, not_selected: count, failed: count, unknown: count }).strict();
export function creatorSaveResultCountsMatch(summary: z.infer<typeof creatorSaveResultSummary>, rows: readonly z.infer<typeof creatorSaveContinuationOutcome>[]): boolean {
  const expected = { total: rows.length, added: 0, updated: 0, existing_skipped: 0, duplicate_skipped: 0, not_selected: 0, failed: 0, unknown: 0 };
  for (const row of rows) expected[row.operation]++;
  return Object.keys(expected).every(key => expected[key as keyof typeof expected] === summary[key as keyof typeof expected]);
}
export type CreatorSaveReview = z.infer<typeof creatorSaveReview>;
export type CreatorSavePreview = z.infer<typeof creatorSavePreview>;
export type CreatorSaveRoute = z.infer<typeof creatorSaveRoute>;
export type CreatorSavePreparedFailure = z.infer<typeof creatorSavePreparedFailure>;
export type CreatorSavePreparedSummary = z.infer<typeof creatorSavePreparedSummary>;
export type CreatorSaveSelection = z.infer<typeof creatorSaveSelection>;
export type CreatorSaveOutcome = z.infer<typeof creatorSaveOutcome>;
export type CreatorSaveResultSummary = z.infer<typeof creatorSaveResultSummary>;
