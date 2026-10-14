import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { z } from "zod";
import { invoke, isRuntimeAvailable } from "./local-runtime";

const profileSchema = z.object({ context_id: z.string(), alias: z.string().nullable() });
const ADDON_IDS = ["onecrew-site-auth", "onecrew-youtube", "onecrew-tiktok", "onecrew-bilibili", "onecrew-tiktok-profile", "onecrew-bilibili-profile", "onecrew-bilibili-video", "onecrew-twitter-identity", "onecrew-twitter-post-identity", "onecrew-twitter-post-content", "onecrew-substack", "onecrew-source-search", "onecrew-browser-search", "onecrew-public-search", "onecrew-bluesky"] as const;
const addonSchema = z.object({
  id: z.enum(ADDON_IDS),
  state: z.enum(["missing", "ready", "conflict", "unavailable"]),
}).strict();
const addonsSchema = z.array(addonSchema).max(ADDON_IDS.length).refine(items => new Set(items.map(item => item.id)).size === items.length);
export const openCliSetupReportSchema = z.object({
  installed: z.boolean(),
  version: z.string().nullable(),
  required_version: z.string().regex(/^\d+\.\d+\.\d+$/),
  command_path: z.string().nullable(),
  command_count: z.number().int().nonnegative().nullable(),
  daemon_running: z.boolean(),
  extension_connected: z.boolean(),
  profiles: z.array(profileSchema),
  search_capabilities: z.array(z.object({
    platform: z.string(), command: z.string(),
    browser_required: z.boolean(), registered: z.boolean(),
  })),
  addons: addonsSchema,
  issue: z.string().nullable(),
});

export type OpenCliSetupReport = z.infer<typeof openCliSetupReportSchema>;
export const OPENCLI_SETUP_QUERY_KEY = ["opencli-setup"] as const;
export const OPENCLI_PROFILE_PREFERENCE_KEY = "onecrew-opencli-profile";

export function openCliVersionIsCompatible(report: OpenCliSetupReport): boolean {
  return report.installed && report.version === report.required_version
    && report.issue !== "version_incompatible" && report.issue !== "version_changed";
}

export async function openOpenCliSetupPage(page: "download" | "extension"): Promise<void> {
  const result = await invoke<unknown>("onecrew_opencli_setup_page_open", { page });
  z.object({ page: z.literal(page), browser: z.literal("chrome"), launch_requested: z.literal(true) }).strict().parse(result);
}

export function canInstallOpenCliAddons(report: Pick<OpenCliSetupReport, "version" | "addons">): boolean {
  return report.version === "1.8.8" && report.addons.length === ADDON_IDS.length
    && report.addons.some(item => item.state === "missing")
    && report.addons.every(item => item.state === "ready" || item.state === "missing");
}

export async function installOpenCliAddons(): Promise<OpenCliSetupReport["addons"]> {
  const result = await invoke<unknown>("onecrew_opencli_addons_install", { approved: true });
  return addonsSchema.refine(items => items.length === ADDON_IDS.length).parse(result);
}

export function useInstallOpenCliAddons() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: installOpenCliAddons,
    retry: false,
    // A failed or interrupted install can leave partial files. Always read back;
    // never optimistically mark ready or automatically retry this local effect.
    onSettled: () => client.invalidateQueries({ queryKey: OPENCLI_SETUP_QUERY_KEY }),
  });
}

export async function readOpenCliSetup(): Promise<OpenCliSetupReport> {
  return openCliSetupReportSchema.parse(await invoke<unknown>("onecrew_opencli_setup_status"));
}

export function useOpenCliSetup() {
  return useQuery({
    queryKey: OPENCLI_SETUP_QUERY_KEY,
    queryFn: readOpenCliSetup,
    enabled: isRuntimeAvailable(),
    retry: false,
    // OpenCLI/Chrome can exit outside OneCrew and emit no runtime event.
    // Poll local status while visible; never perform site reads from this query.
    refetchInterval: 30_000,
    refetchIntervalInBackground: false,
    refetchOnWindowFocus: true,
  });
}

export async function recoverOpenCliBridge(): Promise<OpenCliSetupReport> {
  return openCliSetupReportSchema.parse(await invoke<unknown>("onecrew_opencli_bridge_recover"));
}

export async function repairOpenCliInstallation(): Promise<OpenCliSetupReport> {
  return openCliSetupReportSchema.parse(await invoke<unknown>("onecrew_opencli_installation_repair", { approved: true }));
}

export function useRepairOpenCliInstallation() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: repairOpenCliInstallation, retry: false,
    onSuccess: report => client.setQueryData(OPENCLI_SETUP_QUERY_KEY, report),
    onSettled: () => client.invalidateQueries({ queryKey: OPENCLI_SETUP_QUERY_KEY }),
  });
}

export function useOpenCliRecovery(report: OpenCliSetupReport | undefined) {
  const client = useQueryClient();
  const attempted = useRef(false);
  const recovery = useMutation({
    mutationFn: recoverOpenCliBridge,
    retry: false,
    onSettled: () => client.invalidateQueries({ queryKey: OPENCLI_SETUP_QUERY_KEY }),
  });
  useEffect(() => {
    if (report?.issue === null) attempted.current = false;
    if (report?.issue === "daemon_unavailable" && !attempted.current) {
      attempted.current = true;
      recovery.mutate();
    }
    // Once per outage, including failure: no restart loop from polling.
    // A healthy readback permits automatic recovery of a later outage.
  }, [report?.issue, recovery.mutate]);
  return recovery;
}

export async function checkOpenCliProfile(profile: string): Promise<void> {
  const result = await invoke<unknown>("onecrew_opencli_profile_check", { profile });
  z.object({ profile: z.literal(profile), preflight_passed: z.literal(true) }).parse(result);
}

export function selectedProfileIsConnected(report: OpenCliSetupReport, selected: string | null): boolean {
  return openCliVersionIsCompatible(report) && report.issue === null && report.daemon_running && report.extension_connected
    && !!selected && report.profiles.some(profile => profile.context_id === selected);
}
