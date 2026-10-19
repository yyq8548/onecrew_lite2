import { parseWorkspaceActivity, type WorkspaceActivity } from "@mcp-ui/lib/result-contract/workspace";

type HistoryStorage = Pick<Storage, "getItem" | "setItem">;
const key = (viewId: string) => `onecrew-workspace-history-v1:${viewId}`;
export const validWorkspaceViewId = (value: unknown): value is string =>
  typeof value === "string" && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value);

/** Display snapshots only: never inputs to execution, approval or continuation. */
export function readWorkspaceHistory(storage: HistoryStorage, viewId: string): WorkspaceActivity[] {
  if (!validWorkspaceViewId(viewId)) return [];
  try {
    const raw = storage.getItem(key(viewId));
    if (!raw || raw.length > 2_000_000) return [];
    const values: unknown = JSON.parse(raw);
    if (!Array.isArray(values)) return [];
    const seen = new Set<string>();
    return values.map(value => {
      const activity = parseWorkspaceActivity(value);
      if (!activity || seen.has(activity.campaign_id)) throw new Error("invalid history");
      seen.add(activity.campaign_id);
      return activity;
    });
  } catch { return []; }
}

export function writeWorkspaceHistory(storage: HistoryStorage, viewId: string, history: WorkspaceActivity[]) {
  if (!validWorkspaceViewId(viewId)) return;
  // Sandboxed Hosts can deny storage or exhaust its quota. Keep in-memory cards.
  try { storage.setItem(key(viewId), JSON.stringify(history)); } catch { /* display cache only */ }
}
