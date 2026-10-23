import { z } from "zod";
import type { DiscoveryCampaign } from "./opencli-discovery";

export const stopReasons = ["user_paused", "search_page_left", "page_closed", "request_failed", "revision_changed", "progress_unconfirmed", "busy", "needs_evidence", "search_exhausted", "target_reached", "execution_interrupted"] as const;
export type DiscoveryStopReason = typeof stopReasons[number];
const checkpointSchema = z.object({
  campaign_id: z.string().min(1).max(100), revision: z.number().int().positive(),
  used: z.number().int().nonnegative(),
  reason: z.union([z.enum(stopReasons), z.literal("budget_exhausted")]).nullable().transform(value => value === "budget_exhausted" ? "execution_interrupted" as const : value), recorded_at: z.number().int().positive().max(8_640_000_000_000_000),
});
export type DiscoveryRunCheckpoint = z.infer<typeof checkpointSchema>;
const journalSchema = z.object({ format: z.literal(1), entries: z.array(checkpointSchema).max(20) }).strict();
const KEY = "onecrew-discovery-run-checkpoints-v1";
type StoragePort = Pick<Storage, "getItem" | "setItem">;

/** Display diagnostics only. Never supplies approval, criteria or read authority. */
export function loadRunCheckpoint(storage: Pick<Storage, "getItem">, id: string | null): DiscoveryRunCheckpoint | null {
  try {
    const journal = journalSchema.parse(JSON.parse(storage.getItem(KEY) ?? '{"format":1,"entries":[]}'));
    const entry = journal.entries.find(item => item.campaign_id === id);
    // A previous foreground runner cannot survive a document reload. Preserve
    // uncertainty rather than inventing a click, network error or completion.
    return entry ? { ...entry, reason: entry.reason ?? "execution_interrupted" } : null;
  } catch { return null; }
}
export function loadRunHistory(storage: Pick<Storage, "getItem">, id: string): DiscoveryRunCheckpoint[] {
  try {
    return journalSchema.parse(JSON.parse(storage.getItem(KEY) ?? '{"format":1,"entries":[]}')).entries
      .filter(item => item.campaign_id === id && item.reason !== null).slice(0, 5);
  } catch { return []; }
}
export function saveRunCheckpoint(storage: StoragePort, campaign: Pick<DiscoveryCampaign, "id" | "revision" | "used_calls">, reason: DiscoveryStopReason | null, now = Date.now()): DiscoveryRunCheckpoint | null {
  try {
    const journal = journalSchema.parse(JSON.parse(storage.getItem(KEY) ?? '{"format":1,"entries":[]}'));
    const entry = checkpointSchema.parse({ campaign_id: campaign.id, revision: campaign.revision, used: campaign.used_calls, reason, recorded_at: now });
    const previous = journal.entries[0];
    const updating = previous?.campaign_id === campaign.id && previous.revision === campaign.revision
      && (previous.reason === null || previous.reason === reason);
    const history = (updating ? journal.entries.slice(1) : journal.entries)
      .filter(item => item.campaign_id !== campaign.id || item.reason !== null);
    storage.setItem(KEY, JSON.stringify({ format: 1, entries: [entry, ...history].slice(0, 20) }));
    return entry;
  } catch { return null; }
}
export function checkpointExecution(reason: DiscoveryStopReason | null | undefined): string {
  switch (reason) {
    case "request_failed": case "revision_changed": case "progress_unconfirmed": return "error";
    case "user_paused": case "search_page_left": case "page_closed": case "execution_interrupted": return "paused";
    default: return reason ?? "";
  }
}
export function discoveryStopReason(status: string): DiscoveryStopReason {
  return stopReasons.includes(status as DiscoveryStopReason) ? status as DiscoveryStopReason : "request_failed";
}
