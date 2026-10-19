import { invoke } from "./local-runtime";
import { discoveryDraftProgress, type DraftProgress } from "@onecrew/runtime-contracts/discovery";
export const discoveryDraftProgressKey = (id: string) => ["discovery-draft-progress", id] as const;
export function parseDraftProgress(value: unknown, id: string): DraftProgress | null {
  if (value === null) return null;
  const parsed = discoveryDraftProgress.safeParse(value);
  if (!parsed.success || parsed.data.id !== id) throw new Error("discovery_draft_progress_invalid");
  return parsed.data;
}
export async function getDiscoveryDraftProgress(id: string) {
  return parseDraftProgress(await invoke<unknown>("onecrew_discovery_draft_progress_get", { id }), id);
}
