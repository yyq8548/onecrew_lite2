import { invoke } from "./local-runtime";
import type { DiscoveryCampaign } from "./opencli-discovery";
import type { DiscoveryRunCheckpoint } from "./discovery-run-checkpoint";

export type WorkspaceStage = "ready" | "searching" | "verifying" | "pausing" | "paused" | "finished" | "budget_exhausted" | "needs_evidence" | "error";
export function discoveryRunStageKey(status: string) {
  switch (status) {
    case "unsearched": case "searching": case "completed": case "unauthorized": case "technical_stop": case "risk_control_stop":
      return `opencliPilot.runStages.${status}` as const;
    default: return null;
  }
}
export function discoveryStage(campaign: DiscoveryCampaign | undefined, running: boolean, pausing: boolean, execution: string): WorkspaceStage {
  if (pausing && running) return "pausing";
  if (running) return campaign?.platform_runs.some(run => run.status === "unsearched" || run.status === "searching") ? "searching" : "verifying";
  if (execution === "error") return "error";
  if (execution === "paused") return "paused";
  if (campaign?.platform_runs.some(run => run.status === "searching")) return "searching";
  if (campaign && campaign.max_calls !== null && campaign.used_calls >= campaign.max_calls) return "budget_exhausted";
  if (execution === "needs_evidence") return "needs_evidence";
  if (execution === "search_exhausted") return "needs_evidence";
  if (execution === "target_reached") return "finished";
  return "ready";
}

/** Presence carries no counts, conditions or search authority. */
export function createWorkspacePresence(send = (input: unknown) => invoke<void>("onecrew_discovery_workspace_presence", { input })) {
  const session_id = crypto.randomUUID();
  let sequence = 0;
  let queue: Promise<void> = Promise.resolve();
  return (campaign_id: string | null, stage: WorkspaceStage, activate: boolean, stop?: DiscoveryRunCheckpoint | null, excludedPlatforms: readonly string[] = []) => {
    const input = { session_id, sequence: ++sequence, campaign_id, stage, activate,
      ...(stop?.campaign_id === campaign_id && stop?.reason ? { stop: { reason: stop.reason, recorded_at: stop.recorded_at } } : {}),
      ...(excludedPlatforms.length ? { excluded_platforms: [...excludedPlatforms] } : {}) };
    const result = queue.catch(() => {}).then(() => send(input));
    queue = result;
    return result;
  };
}
