import { invoke } from "./local-runtime";
import type { ExecutionInput, ExecutionStatus } from "./generated/discovery-contracts";
import { validateExecutionStatus } from "./generated/discovery-contracts/validators/ExecutionStatus.js";

export function parseExecution(value: unknown): ExecutionStatus | null {
  if (value === null) return null;
  if (!validateExecutionStatus(value)) throw new Error("discovery_execution_status_invalid");
  if (value.library) {
    const url = new URL(value.library.web_url);
    if (value.library.campaign_id !== value.campaign_id || url.protocol !== "http:" || url.hostname !== "127.0.0.1"
      || !url.port || url.username || url.password || url.search || url.hash
      || url.pathname !== `/library/${value.library.table_id}`) throw new Error("discovery_execution_library_invalid");
  }
  return value;
}
export async function getDiscoveryExecution() {
  return parseExecution(await invoke<unknown>("onecrew_discovery_execution_get"));
}
export const discoveryExecutionKey = (credentialId: string | null) => ["discovery-execution", credentialId] as const;
export async function startDiscoveryExecution(input: ExecutionInput) {
  const result = parseExecution(await invoke<unknown>("onecrew_discovery_execution_start", { input }));
  if (!result || result.campaign_id !== input.campaign_id || result.revision !== input.expected_revision)
    throw new Error("discovery_execution_scope_changed");
  return result;
}
export async function pauseDiscoveryExecution(campaignId: string) {
  const result = parseExecution(await invoke<unknown>("onecrew_discovery_execution_pause", { campaignId }));
  if (result && result.campaign_id !== campaignId) throw new Error("discovery_execution_scope_changed");
  return result;
}
export const executionRunning = (status: ExecutionStatus | null | undefined) =>
  !!status && ["searching", "verifying", "pausing"].includes(status.stage);
