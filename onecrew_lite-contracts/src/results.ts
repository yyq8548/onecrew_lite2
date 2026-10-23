import { z } from "zod";
import { stringifyApplicationJson } from "./json.ts";
import { creatorSaveContinuationOutcome, creatorSaveOutcomes, creatorSaveResultCountsMatch, creatorSaveResultSummary } from "./creator-save.ts";
import { resultByteLength, resultIdentifier, resultText } from "./fields.ts";
import { resultChoiceAnswers } from "./choice.ts";
import { searchPlanCriteria } from "./search-plan.ts";
export { resultIdentifier, resultText } from "./fields.ts";

const safeFieldLabel = resultText.refine(value => {
  const normalized = value.replace(/[^A-Za-z0-9]/g, "").toLowerCase();
  return normalized !== "raw" && !["secret", "password", "passwd", "token", "cookie", "authorization", "apikey", "privatekey", "clientsecret", "rawpayload", "rawresponse", "rawrequest"].some(term => normalized.includes(term));
});
export const callDisposition = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("succeeded") }).strict(),
  z.object({ kind: z.literal("failed"), code: resultIdentifier() }).strict(),
  z.object({ kind: z.literal("unknown"), code: resultIdentifier() }).strict(),
  z.object({ kind: z.literal("not_run"), reason: resultText }).strict(),
]);
export const resultResourceRef = z.object({ kind: z.enum(["creator", "conversation", "connection", "provider_object"]), id: resultIdentifier(256), label: resultText.nullable() }).strict();
export const executedCallOutcome = z.object({ call_id: resultIdentifier(), operation_id: resultIdentifier(), disposition: callDisposition, summary: resultText, resource_refs: z.array(resultResourceRef).max(64) }).strict();
export const executedBatch = z.object({ outcomes: z.array(executedCallOutcome).min(1).max(1000) }).strict().refine(value => new Set(value.outcomes.map(call => call.call_id)).size === value.outcomes.length, "duplicate_call_id");
export const presentationSection = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("creator_save_rows"), rows: creatorSaveOutcomes }).strict(),
  z.object({ kind: z.literal("fields"), heading: resultText.nullable(), fields: z.array(z.object({ label: safeFieldLabel, value: resultText }).strict()).min(1).max(128) }).strict(),
  z.object({ kind: z.literal("items"), heading: resultText.nullable(), items: z.array(z.object({ title: resultText, description: resultText.nullable() }).strict()).min(1).max(128) }).strict(),
  z.object({ kind: z.literal("notice"), heading: resultText.nullable(), body: resultText, tone: z.enum(["neutral", "success", "warning", "critical"]) }).strict(),
]);
const projection = z.object({ kind: resultIdentifier(), title: resultText, summary: resultText.nullable(), sections: z.array(presentationSection) }).strict();
export const presentationProjection = projection.refine(value => value.sections.length <= 32 && resultByteLength(stringifyApplicationJson(value)) <= 64 * 1024, "projection_bytes");
export const creatorSavePresentationProjection = projection.refine(value => value.sections.length <= 82 && resultByteLength(stringifyApplicationJson(value)) <= 4 * 1024 * 1024, "projection_bytes");
export const ownerBatchPresentationProjection = projection.refine(value => value.sections.length <= 656 && resultByteLength(stringifyApplicationJson(value)) <= 32 * 1024 * 1024, "projection_bytes");
export const resultRevision = z.union([z.bigint().min(1n).max(18446744073709551615n), z.number().int().positive().safe().transform(BigInt)]);
export const resultBinding = z.object({
  result_ref: z.string().regex(/^rr_[0-9a-f]{32}$/),
  view: z.object({ identity: z.string().regex(/^rv_[0-9a-f]{32}$/), kind: z.enum(["read_only", "confirmation", "choice"]) }).strict(),
  origin: resultIdentifier(), fingerprint: z.string().regex(/^sha256:[0-9a-f]{64}$/), revision: resultRevision,
}).strict();
export const terminalResult = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("resolved"), code: resultIdentifier(), summary: resultText }).strict(),
  z.object({ kind: z.literal("failed"), code: resultIdentifier(), summary: resultText, retryable: z.boolean() }).strict(),
  z.object({ kind: z.literal("unknown"), code: resultIdentifier(), summary: resultText }).strict(),
]);
export const resultContinuationRequest = z.object({
  message: resultText,
  context: z.discriminatedUnion("kind", [
    z.object({ kind: z.literal("choice"), answers: resultChoiceAnswers }).strict(),
    searchPlanCriteria.extend({ kind: z.literal("search_plan_confirmed"), confirmed_plan_ref: z.uuid() }).strict(),
    z.object({ kind: z.literal("creator_save_batch"), summary: creatorSaveResultSummary, outcomes: z.array(creatorSaveContinuationOutcome).min(1).max(50) }).strict().refine(value => new Set(value.outcomes.map(row => row.row_id)).size === value.outcomes.length && creatorSaveResultCountsMatch(value.summary, value.outcomes), "creator_save_result_summary"),
    z.object({ kind: z.literal("execution"), outcomes: z.array(executedCallOutcome).min(1).max(1000).refine(value => new Set(value.map(call => call.call_id)).size === value.length, "duplicate_continuation_call_id") }).strict(),
    z.object({ kind: z.literal("cancelled") }).strict(),
    z.object({ kind: z.literal("edit_requested") }).strict(),
  ]),
  requires_model_context: z.boolean().default(false),
}).strict();
export type ExecutedBatch = z.infer<typeof executedBatch>;
export type ExecutedCallOutcome = z.infer<typeof executedCallOutcome>;
export type PresentationProjection = z.infer<typeof presentationProjection>;
export type ResultBinding = z.infer<typeof resultBinding>;
export type TerminalResult = z.infer<typeof terminalResult>;
export type ResultContinuationRequest = z.infer<typeof resultContinuationRequest>;
