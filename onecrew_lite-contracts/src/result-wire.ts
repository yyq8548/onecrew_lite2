import { z } from "zod";
import { creatorU64 } from "./creators.ts";
import { creatorSaveReview, creatorSaveSelection } from "./creator-save.ts";
import { resultChoiceAnswers, resultChoiceQuestions } from "./choice.ts";
import { resultIdentifier, resultText } from "./fields.ts";
import { executedBatch, presentationSection, resultBinding, resultContinuationRequest, terminalResult } from "./results.ts";
import { searchPlanEditorDraft, searchPlanPatch } from "./search-plan.ts";

export const wireU64 = creatorU64;
export const wireRevision = wireU64.refine(value => BigInt(value) > 0n);
export const wireResultBinding = resultBinding.extend({ revision: wireRevision });
export const resultPresentationKinds = ["creator_save_batch_confirmation", "creator_save_batch_outcome", "creator_batch_confirmation", "creator_batch_outcome", "outreach_confirmation", "outreach_outcome", "email_confirmation", "email_outcome", "search_plan_editor", "outreach_draft", "record_preview", "simple_operation", "read_only", "choice", "batch_result"] as const;
const wireNumber = (value: bigint): number | bigint => value <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(value) : value;
export const wireCreatorSaveReview = creatorSaveReview.transform(review => ({ ...review, rows: review.rows.map(row => ({ ...row, preview: { ...row.preview, expires_at: wireNumber(row.preview.expires_at), follower_counts: Object.fromEntries(Object.entries(row.preview.follower_counts).map(([key, value]) => [key, wireNumber(value)])) } })) }));
export const wireContinuation = z.object({ state: z.enum(["not_requested", "pending", "dispatching", "delivered", "paused", "delivery_unknown"]), continuation_revision: wireRevision, dispatch_started_at_ms: wireU64.nullable(), dispatch_not_after_ms: wireU64.nullable() }).strict();
export const wireResultState = z.union([
  z.object({ kind: z.literal("available") }).strict(),
  z.object({ kind: z.literal("submitted"), action: z.discriminatedUnion("kind", [z.object({ kind: z.literal("confirmed") }).strict(), z.object({ kind: z.literal("selected"), answers: resultChoiceAnswers }).strict()]) }).strict(),
  terminalResult,
]);
export const wirePendingAction = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("creator_save_batch"), review: wireCreatorSaveReview }).strict(),
  z.object({ kind: z.literal("search_plan"), draft: searchPlanEditorDraft, draft_digest: z.string().regex(/^sha256:[0-9a-f]{64}$/) }).strict(),
  z.object({ kind: z.literal("confirm"), prompt: resultText, label: resultText }).strict(),
  z.object({ kind: z.literal("choice"), questions: resultChoiceQuestions }).strict(),
]);
export const wirePresentation = z.object({ kind: z.enum(resultPresentationKinds), title: resultText, summary: resultText.nullable(), sections: z.array(presentationSection).max(656) }).strict();
export const wireResultSnapshot = z.object({ locale: z.enum(["en-US", "zh-CN"]), binding: wireResultBinding, presented: z.literal(true), batch: executedBatch, presentation: wirePresentation, completed_sections_count: z.number().int().nonnegative().safe(), pending_action: wirePendingAction.nullable(), state: wireResultState, continuation: wireContinuation }).strict();
export const wireClaimedContinuation = z.object({ binding: wireResultBinding, continuation_revision: wireRevision, lease_id: z.string().regex(/^rcl_[0-9a-f]{32}$/), request: resultContinuationRequest, dispatch_started_at_ms: wireU64, dispatch_not_after_ms: wireU64 }).strict();
export const wireResultFailure = z.union([
  z.object({ code: z.enum(["revision_conflict", "continuation_revision_conflict"]), current_revision: wireRevision }).strict(),
  z.object({ code: z.enum(["absent", "expired", "view_mismatch", "origin_mismatch", "fingerprint_mismatch", "continuation_lease_mismatch", "invalid_input", "search_plan_payload_too_large", "invalid_state", "replay", "capacity", "unavailable"]) }).strict(),
]);
export const wireResultInput = z.discriminatedUnion("action", [
  z.object({ action: z.enum(["read_current", "confirm", "cancel"]), binding: wireResultBinding }).strict(),
  z.object({ action: z.literal("confirm_creator_save_batch"), binding: wireResultBinding, selections: z.array(creatorSaveSelection).min(1).max(50) }).strict(),
  z.object({ action: z.literal("revise_search_plan"), binding: wireResultBinding, patch: searchPlanPatch }).strict(),
  z.object({ action: z.literal("select"), binding: wireResultBinding, answers: resultChoiceAnswers }).strict(),
  z.object({ action: z.literal("request_edit"), input: z.object({ binding: wireResultBinding, instruction: resultText.refine(value => new TextEncoder().encode(value).length <= 2048) }).strict() }).strict(),
  z.object({ action: z.literal("claim_continuation"), input: z.object({ binding: wireResultBinding, expected_continuation_revision: wireRevision, mode: z.enum(["pending", "retry_paused"]) }).strict() }).strict(),
  z.object({ action: z.literal("complete_continuation"), input: z.object({ binding: wireResultBinding, expected_continuation_revision: wireRevision, lease_id: z.string().regex(/^rcl_[0-9a-f]{32}$/), outcome: z.enum(["delivered", "paused", "delivery_unknown"]) }).strict() }).strict(),
]);
export type WireResultSnapshot = z.infer<typeof wireResultSnapshot>;
export type WireResultBinding = z.infer<typeof wireResultBinding>;
export type WireResultInput = z.infer<typeof wireResultInput>;
export type WireResultFailure = z.infer<typeof wireResultFailure>;
export type WireClaimedContinuation = z.infer<typeof wireClaimedContinuation>;
export const compareU64 = (left: number | bigint, right: number | bigint): number => BigInt(left) < BigInt(right) ? -1 : BigInt(left) > BigInt(right) ? 1 : 0;
export const isU64 = (value: unknown): value is number | bigint => wireU64.safeParse(value).success;
