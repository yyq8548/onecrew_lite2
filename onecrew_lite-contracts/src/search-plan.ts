import { z } from "zod";
import { isWellFormedUnicode, stringifyApplicationJson } from "./json.ts";
import { resultByteLength } from "./fields.ts";

export const SEARCH_PLAN_MAX_BYTES = 512 * 1024;
export const SEARCH_PLAN_MAX_SCOPES = 200;
export const SEARCH_PLAN_MAX_ACTIVE = 1024;
export const SEARCH_PLAN_TTL_SECONDS = 3600;
const text = z.string().refine(isWellFormedUnicode);
const criteriaDescription = text.refine(value => !!value.trim() && resultByteLength(value) <= 8192, "invalid_description");
const criteriaKeywords = z.array(text.refine(value => !!value.length && value.trim() === value && resultByteLength(value) <= 512 && !/\p{Cc}/u.test(value), "invalid_keyword")).max(50, "too_many_keywords").refine(values => new Set(values).size === values.length, "invalid_keyword");
export const searchPlanConnection = z.discriminatedUnion("kind", [
  z.strictObject({ kind: z.literal("operator_account"), toolkit: text, operator_account_id: text }),
  z.strictObject({ kind: z.literal("service_credential"), toolkit: text, credential_id: text }),
]);
export const searchPlanScope = z.strictObject({ operation_id: text.refine(value => !!value.trim()), schema_revision: text.refine(value => !!value.trim()), connection: searchPlanConnection, arguments: z.record(z.string(), z.unknown()) });
export const searchPlanCriteria = z.strictObject({ description: criteriaDescription, keywords: criteriaKeywords });
export const searchPlanEditorDraft = z.strictObject({ description: criteriaDescription, keywords: criteriaKeywords, scopes: z.array(searchPlanScope).min(1).max(SEARCH_PLAN_MAX_SCOPES) }).refine(value => resultByteLength(stringifyApplicationJson(value)) <= SEARCH_PLAN_MAX_BYTES, "search_plan_payload_too_large");
export const searchPlanPatch = z.strictObject({ description: text.nullable().optional(), keywords: z.array(text).nullable().optional() });
export type SearchPlanScope = z.infer<typeof searchPlanScope>;
export type SearchPlanEditorDraft = z.infer<typeof searchPlanEditorDraft>;
export type SearchPlanConnection = z.infer<typeof searchPlanConnection>;
export function applySearchPlanPatch(raw: unknown, patch: unknown): SearchPlanEditorDraft {
  const plan = searchPlanEditorDraft.parse(raw), update = searchPlanPatch.parse(patch);
  return searchPlanEditorDraft.parse({ description: update.description ?? plan.description, keywords: update.keywords ?? plan.keywords, scopes: plan.scopes });
}
