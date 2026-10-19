export const workspaceStages = ["ready", "searching", "verifying", "pausing", "paused", "finished", "budget_exhausted", "needs_evidence", "error"] as const;
export type WorkspaceActivity = {
  campaign_id: string; brief: string; stage: typeof workspaceStages[number];
  qualified: number; pending: number; target: number; attempted: number; total: number; used: number; max: number | null;
  login_required: number; read_errors: number; current_read_stops?: number; restricted: number; stale: boolean;
  read_operation_stops?: { platform: string; profile_ref: string; operation: string; code: string }[];
  updated_at?: number;
  library_sync_pending?: boolean;
  deferred_scores?: number;
  pending_breakdown?: { awaiting_score: number; needs_content: number; needs_identity: number; read_blocked: number; insufficient_after_read: number; needs_requirement: number };
  failure_code?: string;
  library_sync_error?: string;
  library_synced_at?: number;
  failure_phase?: "read" | "score" | "admission" | "readback" | "execution";
  stop?: { reason: typeof workspaceStopReasons[number]; recorded_at: number };
};
export const workspaceStopReasons = ["user_paused", "search_page_left", "page_closed", "request_failed", "revision_changed", "progress_unconfirmed", "busy", "needs_evidence", "search_exhausted", "target_reached", "budget_exhausted", "execution_interrupted"] as const;
export function parseWorkspaceActivity(value: unknown): WorkspaceActivity | null {
  if (value === null) return null;
  if (typeof value !== "object" || !value || Array.isArray(value)) throw new Error("invalid workspace activity");
  const data = value as Record<string, unknown>;
  if (Object.keys(data).filter(key => !["stop", "updated_at", "library_sync_pending", "deferred_scores", "pending_breakdown", "failure_phase", "failure_code", "library_sync_error", "library_synced_at", "current_read_stops", "read_operation_stops"].includes(key)).sort().join() !== "attempted,brief,campaign_id,login_required,max,pending,qualified,read_errors,restricted,stage,stale,target,total,used"
    || typeof data.campaign_id !== "string" || !data.campaign_id || typeof data.brief !== "string"
    || !workspaceStages.includes(data.stage as WorkspaceActivity["stage"]) || typeof data.stale !== "boolean") throw new Error("invalid workspace activity");
  for (const key of ["qualified", "pending", "target", "attempted", "total", "used", "login_required", "read_errors", "restricted"]) {
    if (typeof data[key] !== "number" || !Number.isSafeInteger(data[key]) || (data[key] as number) < 0) throw new Error("invalid workspace count");
  }
  if (data.current_read_stops !== undefined && (typeof data.current_read_stops !== "number" || !Number.isSafeInteger(data.current_read_stops) || data.current_read_stops < 0)) throw new Error("invalid current read stops");
  if (data.read_operation_stops !== undefined && (!Array.isArray(data.read_operation_stops) || data.read_operation_stops.some(stop =>
    !stop || typeof stop !== "object" || Array.isArray(stop) || Object.keys(stop).sort().join() !== "code,operation,platform,profile_ref"
    || ["platform", "profile_ref", "operation", "code"].some(key => typeof stop[key] !== "string" || !stop[key])
  ))) throw new Error("invalid read operation stops");
  if (data.max !== null && (typeof data.max !== "number" || !Number.isSafeInteger(data.max) || data.max < 1)) throw new Error("invalid workspace limit");
  if ((data.attempted as number) > (data.total as number) || (data.max !== null && (data.used as number) > (data.max as number))) throw new Error("invalid workspace bounds");
  if ("updated_at" in data && (typeof data.updated_at !== "number" || !Number.isSafeInteger(data.updated_at) || data.updated_at <= 0)) throw new Error("invalid workspace timestamp");
  if ("library_sync_pending" in data && typeof data.library_sync_pending !== "boolean") throw new Error("invalid library sync status");
  if ("deferred_scores" in data && (typeof data.deferred_scores !== "number" || !Number.isSafeInteger(data.deferred_scores) || data.deferred_scores < 0)) throw new Error("invalid deferred score count");
  if (data.pending_breakdown !== undefined) {
    const counts = data.pending_breakdown;
    if (!counts || typeof counts !== "object" || Array.isArray(counts)
      || Object.keys(counts).sort().join() !== "awaiting_score,insufficient_after_read,needs_content,needs_identity,needs_requirement,read_blocked"
      || Object.values(counts).some(value => typeof value !== "number" || !Number.isSafeInteger(value) || value < 0)) throw new Error("invalid pending review counts");
  }
  if ("failure_phase" in data && !["read", "score", "admission", "readback", "execution"].includes(data.failure_phase as string)) throw new Error("invalid execution phase");
  for (const key of ["failure_code", "library_sync_error"]) {
    if (key in data && (typeof data[key] !== "string" || !/^[a-z][a-z_]{0,95}$/.test(data[key] as string))) throw new Error("invalid diagnostic code");
  }
  if ("library_synced_at" in data && (typeof data.library_synced_at !== "number" || !Number.isSafeInteger(data.library_synced_at) || data.library_synced_at <= 0 || data.library_synced_at > 8_640_000_000_000_000)) throw new Error("invalid sync timestamp");
  if ("stop" in data) {
    const stop = data.stop as WorkspaceActivity["stop"];
    if (!stop || typeof stop !== "object" || Array.isArray(stop) || Object.keys(stop).sort().join() !== "reason,recorded_at"
      || !workspaceStopReasons.includes(stop.reason) || !Number.isSafeInteger(stop.recorded_at) || stop.recorded_at <= 0 || stop.recorded_at > 8_640_000_000_000_000) throw new Error("invalid workspace stop");
  }
  return data as WorkspaceActivity;
}
