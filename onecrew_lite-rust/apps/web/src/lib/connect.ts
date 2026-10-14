import { useQuery } from "@tanstack/react-query";
import { invoke, isRuntimeAvailable } from "./local-runtime";
import type { LocalServiceStatus } from "./generated/creator-sync-contracts";
import { validateLocalServiceStatus } from "./generated/creator-sync-contracts/validators/LocalServiceStatus.js";

export const SERVICE_STATUS_QUERY_KEY = ["local-service-status"] as const;

export async function readServiceStatus(): Promise<LocalServiceStatus> {
  const result = await invoke<unknown>("service_status");
  if (!validateLocalServiceStatus(result)) throw new Error("invalid_service_status");
  return result;
}

export function useServiceStatus() {
  return useQuery({
    queryKey: SERVICE_STATUS_QUERY_KEY,
    queryFn: readServiceStatus,
    enabled: isRuntimeAvailable(),
    // Local status only; this query never logs in or calls a provider.
    refetchInterval: 5_000,
  });
}

export async function logoutFromService(): Promise<void> {
  await invoke("onecrew_logout");
}

/** Explicit local Codex inspection. This never installs/enables a plugin. */
export async function refreshCodexPluginStatus(): Promise<LocalServiceStatus["plugin"]> {
  return invoke<LocalServiceStatus["plugin"]>("onecrew_codex_plugin_status_refresh");
}
