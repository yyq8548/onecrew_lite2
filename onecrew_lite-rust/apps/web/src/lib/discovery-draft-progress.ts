import { invoke } from "./local-runtime";
import type { DraftProgress } from "./generated/discovery-contracts";
import { validateDraftProgress } from "./generated/discovery-contracts/validators/DraftProgress.js";
export const discoveryDraftProgressKey = (id: string) => ["discovery-draft-progress", id] as const;
export function parseDraftProgress(value: unknown, id: string): DraftProgress | null {
  if (value === null) return null;
  if (!validateDraftProgress(value) || value.id !== id) throw new Error("discovery_draft_progress_invalid");
  return value;
}
export async function getDiscoveryDraftProgress(id: string) {
  return parseDraftProgress(await invoke<unknown>("onecrew_discovery_draft_progress_get", { id }), id);
}
