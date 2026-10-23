import { z } from "zod";

const code = z.string().regex(/^[a-zA-Z][a-zA-Z0-9_.:-]{0,95}$/u);
export const repairAction = z.enum(["refresh_connections", "repair_components", "recover_bridge", "sync_results", "retry_plan"]);
export const repairBeginInput = z.strictObject({
  request_id: z.uuid(), page: z.enum(["connections", "search"]),
  campaign_id: z.string().regex(/^[A-Za-z\d_-]{1,128}$/u).nullish().transform(value => value ?? null),
  client_errors: z.array(z.strictObject({ command: code, code, at: z.number().int().nonnegative() })).max(8).default([]),
  failed_draft: z.record(z.string(), z.unknown()).nullable().default(null),
});
export const repairGetInput = z.strictObject({ repair_id: z.uuid() });
export const repairApplyInput = repairGetInput.extend({ action: repairAction });
export const repairFinishInput = repairGetInput.extend({ summary: z.string().trim().min(1).max(2000), unresolved: z.boolean().default(false) });
export const repairCheck = z.strictObject({
  name: z.enum(["service", "data", "plugin", "browser", "library", "search", "plan", "client"]),
  state: z.enum(["ready", "issue", "unavailable"]), code: z.string().nullable(),
});
export const repairStatus = z.strictObject({
  id: z.uuid(), request_id: z.uuid(), page: z.enum(["connections", "search"]),
  state: z.enum(["checking", "awaiting_model", "repairing", "completed", "needs_action", "failed"]),
  created_at: z.number(), notified_at: z.number().nullable(), received_at: z.number().nullable(),
  checks: z.array(repairCheck),
  steps: z.array(z.strictObject({ action: repairAction, state: z.enum(["running", "completed", "failed"]), code: z.string().nullable() })),
  summary: z.string().nullable(), failure_code: z.string().nullable(), draft_id: z.string().nullable(),
});
export type RepairStatus = z.infer<typeof repairStatus>;
export type RepairAction = z.infer<typeof repairAction>;
