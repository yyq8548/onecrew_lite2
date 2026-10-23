import { z } from "zod";

export const managedBrowserBinding = z.strictObject({ binding_id: z.uuid(), context_id: z.string().min(1) });

export const managedBrowserConnection = z.strictObject({
  flow_id: z.uuid(),
  phase: z.enum(["ready_to_begin", "starting", "awaiting_user_in_app", "finalizing", "completed", "canceled", "failed"]),
  binding_id: z.uuid().optional(),
  context_id: z.string().min(1).optional(),
});
export type ManagedBrowserConnection = z.infer<typeof managedBrowserConnection>;
