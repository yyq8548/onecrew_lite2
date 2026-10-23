/* Generated. Do not edit. */

export interface ExecutionStatus {
  campaign_id: string;
  deferred_hits: {
    reason: string;
    search_hit_id: string;
  }[];
  deferred_scores: number;
  excluded_platforms: string[];
  failure_code: string | null;
  failure_phase: string | null;
  library: {
    campaign_id: string;
    inserted_rows: number;
    table_id: string;
    total_hits: number;
    web_url: string;
  } | null;
  reason: string | null;
  revision: number;
  stage:
    | "ready"
    | "searching"
    | "verifying"
    | "pausing"
    | "paused"
    | "finished"
    | "budget_exhausted"
    | "needs_evidence"
    | "error";
  sync_error: string | null;
  sync_pending: boolean;
  synced_at: number | null;
  updated_at: number;
  used: number;
}
