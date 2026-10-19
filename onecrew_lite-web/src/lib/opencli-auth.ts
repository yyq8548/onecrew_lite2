import { z } from "zod";
import { invoke } from "./local-runtime";
import { openCliSearchNeedsLoginCheck } from "./opencli-search-access";

const observationSchema = z.object({
  platform: z.string().min(1),
  profile_ref: z.string().min(1),
  status: z.enum(["logged_in", "session_detected", "not_logged_in", "unknown", "error", "public_read"]),
  checked_at: z.number().int().nonnegative(),
  method: z.enum(["quick", "full", "skipped"]),
  issue: z.enum(["auth_signal_unavailable", "auth_probe_failed", "auth_probe_rate_limited", "auth_probe_navigation_failed", "auth_probe_timeout", "auth_probe_unavailable", "auth_scope_mismatch", "auth_addon_required", "entitlement_unverified"]).nullable(),
}).strict();
const savedSchema = z.array(observationSchema).max(57);
// v1 called a cookie-only result logged_in. Do not reuse it as identity proof.
const STORAGE_PREFIX = "onecrew:opencli-auth-observations:v2:";
export const OPENCLI_AUTH_CHANGED = "onecrew:opencli-auth-changed";

export type OpenCliAuthObservation = z.infer<typeof observationSchema>;

export async function readOpenCliAuthObservations(profile: string): Promise<Record<string, OpenCliAuthObservation>> {
  const observations = savedSchema.parse(await invoke<unknown>("onecrew_opencli_auth_observations", { profile }));
  return Object.fromEntries(observations.filter(item => item.profile_ref === profile).map(item => [item.platform, item]));
}

export async function openOpenCliLoginPage(profile: string, platform: string): Promise<void> {
  const result = z.object({ profile: z.string(), platform: z.string(), opened: z.literal(true) }).strict()
    .parse(await invoke<unknown>("onecrew_opencli_login_page_open", { profile, platform }));
  if (result.profile !== profile || result.platform !== platform) throw new Error("opencli_login_target_mismatch");
}

/** A provider read happens only after the customer requests a login check. */
export async function checkOpenCliSiteAuth(profile: string, platform: string): Promise<OpenCliAuthObservation> {
  const result = observationSchema.parse(await invoke<unknown>("onecrew_opencli_site_auth_check", { profile, platform }));
  if (result.profile_ref !== profile || result.platform !== platform) throw new Error("opencli_auth_target_mismatch");
  return result;
}

export async function checkOpenCliSitesAuth(profile: string, platforms: string[], signal: AbortSignal,
  onResult: (platform: string, observation: OpenCliAuthObservation | null, completed: number) => void): Promise<void> {
  // OpenCLI shares one browser bridge: check serially, without automatic retries.
  for (const [index, platform] of [...new Set(platforms)].entries()) {
    if (signal.aborted) break;
    let observation: OpenCliAuthObservation | null = null;
    try { observation = await checkOpenCliSiteAuth(profile, platform); } catch {
      // Persist this failed check, replacing stale success for this exact target.
      observation = {platform, profile_ref: profile, status: "error", checked_at: Math.floor(Date.now() / 1000), method: "full", issue: "auth_probe_failed"};
    }
    if (signal.aborted) break;
    onResult(platform, observation, index + 1);
  }
}

function storageKey(credentialId: string | null, profile: string | null): string | null {
  if (!credentialId || !profile) return null;
  return `${STORAGE_PREFIX}${encodeURIComponent(credentialId)}:${encodeURIComponent(profile)}`;
}

/** Cached observations are historical evidence, never a live login assertion. */
export function loadOpenCliAuthObservations(credentialId: string | null, profile: string | null): Record<string, OpenCliAuthObservation> {
  const key = storageKey(credentialId, profile);
  if (!key || typeof window === "undefined") return {};
  try {
    const raw = window.localStorage.getItem(key);
    if (!raw) return {};
    const items = savedSchema.parse(JSON.parse(raw) as unknown);
    return Object.fromEntries(items.filter(item => item.profile_ref === profile).map(item => [item.platform, item]));
  } catch { return {}; }
}

export function saveOpenCliAuthObservation(credentialId: string | null, observation: OpenCliAuthObservation): void {
  const key = storageKey(credentialId, observation.profile_ref);
  if (!key || typeof window === "undefined") return;
  const previous = loadOpenCliAuthObservations(credentialId, observation.profile_ref);
  if (previous[observation.platform]?.checked_at > observation.checked_at) return;
  const items = Object.values({ ...previous, [observation.platform]: observation });
  if (items.length > 57) return;
  try { window.localStorage.setItem(key, JSON.stringify(items)); window.dispatchEvent(new Event(OPENCLI_AUTH_CHANGED)); }
  catch { /* The current-page result remains visible when browser storage is unavailable. */ }
}

/** Display history survives service restarts; preflight deliberately reads live receipts only. */
export function displayOpenCliAuthObservations(credentialId: string | null, profile: string | null,
  live: Record<string, OpenCliAuthObservation>): Record<string, OpenCliAuthObservation> {
  const history = loadOpenCliAuthObservations(credentialId, profile);
  for (const item of Object.values(live)) {
    if (item.profile_ref === profile && (!history[item.platform] || item.checked_at >= history[item.platform].checked_at)) {
      history[item.platform] = item;
    }
  }
  return history;
}

/** A negative check excludes only its exact account/browser scope. Unknown is not signed out. */
export function signedOutPlatforms(observations: Record<string, OpenCliAuthObservation>, profile: string): string[] {
  return Object.values(observations).filter(item => item.profile_ref === profile && item.status === "not_logged_in")
    .map(item => item.platform);
}

/** Verified self-account observations expire; cookie-only and future timestamps do not admit reads. */
export function isFreshOpenCliAuthObservation(observation: OpenCliAuthObservation | undefined, profile: string,
  nowSeconds = Math.floor(Date.now() / 1000)): boolean {
  return !!observation && observation.profile_ref === profile && observation.status === "logged_in"
    && observation.method === "full" && observation.issue === null
    && observation.checked_at <= nowSeconds && nowSeconds - observation.checked_at <= 600;
}

/** Unknown/session-only evidence never admits a login-dependent search. */
export function excludedSearchPlatforms(observations: Record<string, OpenCliAuthObservation>, profile: string,
  capabilities: readonly { platform: string; browser_required: boolean }[], nowSeconds = Math.floor(Date.now() / 1000)): string[] {
  return capabilities.filter(({ platform, browser_required }) => {
    if (!openCliSearchNeedsLoginCheck(platform, browser_required)) return false;
    const observation = observations[platform];
    if (observation?.profile_ref === profile && observation.status === "not_logged_in") return true;
    return observation?.platform !== platform || !isFreshOpenCliAuthObservation(observation, profile, nowSeconds);
  }).map(item => item.platform);
}

/** Refresh stale/missing login proof before starting, without searches or automatic retries. */
export async function preflightOpenCliSearch(credentialId: string | null, profile: string,
  capabilities: readonly {platform: string; browser_required: boolean}[]): Promise<string[]> {
  const observations = await readOpenCliAuthObservations(profile);
  for (const item of capabilities) {
    if (!openCliSearchNeedsLoginCheck(item.platform, item.browser_required)
      || (observations[item.platform]?.platform === item.platform && isFreshOpenCliAuthObservation(observations[item.platform], profile))) continue;
    try { observations[item.platform] = await checkOpenCliSiteAuth(profile, item.platform); }
    catch { observations[item.platform] = {platform: item.platform, profile_ref: profile, status: "error",
      checked_at: Math.floor(Date.now() / 1000), method: "full", issue: "auth_probe_failed"}; }
    saveOpenCliAuthObservation(credentialId, observations[item.platform]);
  }
  return excludedSearchPlatforms(observations, profile, capabilities);
}

export function clearOpenCliAuthObservations(credentialId: string | null): void {
  if (!credentialId || typeof window === "undefined") return;
  const prefixes = [STORAGE_PREFIX, "onecrew:opencli-auth-observations:v1:"]
    .map(prefix => `${prefix}${encodeURIComponent(credentialId)}:`);
  try {
    for (let index = window.localStorage.length - 1; index >= 0; index -= 1) {
      const key = window.localStorage.key(index);
      if (key && prefixes.some(prefix => key.startsWith(prefix))) window.localStorage.removeItem(key);
    }
  } catch { /* Logout still completes when local storage is unavailable. */ }
}
