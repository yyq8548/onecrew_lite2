import { z } from "zod";
import { stringifyApplicationJson } from "../contracts/application-json.ts";

export const mcpEntryId = z.string().min(1).max(128).regex(/^[a-z0-9._-]+$/u);
export const mcpToolkit = z.enum(["email", "instagram", "onecrew", "telegram", "x", "youtube"]);
export const mcpEffect = z.enum(["read_local", "read_open_world", "write_local", "delete_local", "sync_open_world", "external_write", "external_step"]);
const unique = <T>(values: readonly T[]) => new Set(values).size === values.length;
export const mcpConnection = z.discriminatedUnion("kind", [
  z.strictObject({ kind: z.literal("operator_account"), toolkit: z.enum(["x", "instagram", "telegram"]), operator_account_id: z.string() }),
  z.strictObject({ kind: z.literal("service_credential"), toolkit: z.literal("youtube"), credential_id: z.string() }),
  z.strictObject({ kind: z.literal("mailbox"), toolkit: z.literal("email"), mailbox_ref_id: z.string() }),
]);
export const mcpExecutionCall = z.strictObject({ call_id: mcpEntryId, operation_id: mcpEntryId, schema_revision: z.string(), authority: z.strictObject({ kind: z.literal("direct") }), connection: mcpConnection.nullish().transform(value => value ?? null), arguments: z.record(z.string(), z.unknown()) });
export type McpExecutionCall = z.infer<typeof mcpExecutionCall>;
export const mcpExecuteInput = z.strictObject({ calls: z.array(mcpExecutionCall).min(1).max(8).refine(calls => unique(calls.map(call => call.call_id)), "calls must contain 1 to 8 unique call_id values"), presentation: z.enum(["never", "if_required", "requested"]) }).refine(value => Buffer.byteLength(stringifyApplicationJson(value)) <= 1024 * 1024, "mcp_execution_batch_too_large");
export type McpExecuteInput = z.infer<typeof mcpExecuteInput>;
export const mcpToolGetInput = z.strictObject({ operation_ids: z.array(mcpEntryId).min(1).max(8).refine(unique, "operation_ids must contain 1 to 8 unique values"), include_output_schema: z.boolean().default(false) });
export const mcpConnectionListInput = z.strictObject({ toolkits: z.array(mcpToolkit).max(8).refine(unique).default([]) });
export const mcpToolSearchInput = z.strictObject({ query: z.string().refine(value => Buffer.byteLength(value) >= 1 && Buffer.byteLength(value) <= 256, "search query must contain between 1 and 256 bytes"), toolkits: z.array(mcpToolkit).max(8).refine(unique).default([]), effects: z.array(mcpEffect).max(7).refine(unique).default([]), limit: z.number().int().min(1).max(8).nullish().transform(value => value ?? 8) });
export const mcpResultPresentInput = z.strictObject({ result_ref: z.string().regex(/^rr_[a-f0-9]{32}$/u), locale: z.enum(["en-US", "zh-CN"]) });

export interface McpFailurePresentation {
  code: string; phase: "before_invocation" | "after_invocation"; retry_safe: boolean;
  effect: "not_invoked" | "known_no_effect" | "applied" | "unknown";
  recovery: { kind: "prepare_search_plan"; operation_id: string } | { kind: "correct_input" } | { kind: "confirm"; parameter: string } | { kind: "reuse_idempotency_key" } | { kind: "read_status"; reference: string } | { kind: "reconnect" } | { kind: "retry_later"; minimum_delay_seconds: number; max_attempts: number } | { kind: "contact_support"; audit_id: string } | null;
  provider_status?: number; provider_evidence?: unknown; message?: string; dm_failure?: unknown;
}
export type McpExecutionOutcome =
  | { status: "completed"; call_id: string; operation_id: string; result: unknown }
  | { status: "input_required"; call_id: string; operation_id: string; input_kind: "confirmation" | "choice"; review_summary?: unknown }
  | { status: "failed" | "unknown"; call_id: string; operation_id: string; failure: McpFailurePresentation; review_summary?: unknown }
  | { status: "skipped"; call_id: string; operation_id: string; reason: string };
export interface McpExecutionResult { batch_id: string; outcomes: McpExecutionOutcome[]; result_ref: string | null; presentation_failure: { code: "projection_unavailable" | "capacity" | "store_rejected" | "store_unavailable" } | null }
