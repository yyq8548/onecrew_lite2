import { invoke } from "./local-runtime";
import type { ExecutionInput } from "./generated/discovery-contracts";
import { discoveryExecutionStatus, type ExecutionStatus } from "@onecrew/runtime-contracts/discovery";
import { discoveryReadFailures } from "@onecrew/runtime-contracts/status";

export function parseExecution(value: unknown): ExecutionStatus | null {
  if (value === null) return null;
  const parsed = discoveryExecutionStatus.safeParse(value);
  if (!parsed.success) throw new Error("discovery_execution_status_invalid");
  const status = parsed.data;
  if (status.library) {
    const url = new URL(status.library.web_url);
    if (status.library.campaign_id !== status.campaign_id || url.protocol !== "http:" || url.hostname !== "127.0.0.1"
      || !url.port || url.username || url.password || url.search || url.hash
      || url.pathname !== `/library/${status.library.table_id}`) throw new Error("discovery_execution_library_invalid");
  }
  return status;
}
export async function getDiscoveryExecution() {
  return parseExecution(await invoke<unknown>("onecrew_discovery_execution_get"));
}
export async function getDiscoveryReadFailures(campaignId: string) {
  return discoveryReadFailures.parse(await invoke<unknown>("onecrew_discovery_read_failures", { campaignId }));
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
