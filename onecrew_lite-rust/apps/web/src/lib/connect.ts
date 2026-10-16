import { useQuery } from "@tanstack/react-query";
import { invoke, isRuntimeAvailable } from "./local-runtime";
import { localServiceStatus, type LocalServiceStatus } from "@onecrew/runtime-contracts/status";

export const SERVICE_STATUS_QUERY_KEY = ["local-service-status"] as const;

export async function readServiceStatus(): Promise<LocalServiceStatus> {
  const result = await invoke<unknown>("service_status");
  const parsed = localServiceStatus.safeParse(result);
  if (!parsed.success) throw new Error("invalid_service_status");
  return parsed.data;
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
  const parsed = localServiceStatus.shape.plugin.safeParse(await invoke<unknown>("onecrew_codex_plugin_status_refresh"));
  if (!parsed.success) throw new Error("invalid_plugin_status");
  return parsed.data;
}
