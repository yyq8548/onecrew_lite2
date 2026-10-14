import type { LocalServiceStatus } from "./generated/creator-sync-contracts";

/** Registration failures the service reports, repaired only by `onecrew plugin install`. */
export const PLUGIN_REPAIR_DIAGNOSTICS = [
  "codex_marketplace_source_missing",
  "codex_marketplace_source_relocated",
] as const;

export type PluginRepairDiagnostic = (typeof PLUGIN_REPAIR_DIAGNOSTICS)[number];

export function pluginRepairDiagnostic(plugin: LocalServiceStatus["plugin"]): PluginRepairDiagnostic | null {
  return PLUGIN_REPAIR_DIAGNOSTICS.find((code) => code === plugin.diagnostic_code) ?? null;
}
