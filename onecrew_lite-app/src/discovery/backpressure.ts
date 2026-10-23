export function discoveryShouldHold(held: boolean, activePending: number): boolean {
  if (!Number.isSafeInteger(activePending) || activePending < 0) throw new Error("discovery_pending_count_invalid");
  return held ? activePending > 5 : activePending >= 10;
}

export interface DiscoveryPipelineStatus {
  active_pending: number;
  queued_scores: number;
  running_scores: number;
  discovery_held: boolean;
  contact_pending: boolean;
}
