import { isFreshOpenCliAuthObservation } from "./opencli-auth";
import type { OpenCliAuthObservation } from "./opencli-auth";
import type { OpenCliSetupReport } from "./opencli-setup";
import { openCliSearchAccess, openCliAccountNeedsLoginCheck } from "./opencli-search-access";

type Capability = OpenCliSetupReport["search_capabilities"][number];

/** A failed check or detected session is not a request to sign in again. */
export function openCliPlatformNeedsSignIn(item: Capability, profile: string | null,
  observation: OpenCliAuthObservation | undefined, failed: boolean): boolean {
  if (!openCliAccountNeedsLoginCheck(item.platform, item.browser_required) || failed) return false;
  if (!profile || observation?.profile_ref !== profile || observation.platform !== item.platform) return true;
  return observation.status === "not_logged_in" || observation.status === "public_read";
}

/** Preserve the catalog order within each group; never mutate the live report. */
export function orderOpenCliPlatforms(capabilities: readonly Capability[]): Capability[] {
  const rank = (item: Capability) => {
    const access = openCliSearchAccess(item.platform, item.browser_required);
    return access === "public_read" ? 2 : openCliAccountNeedsLoginCheck(item.platform, item.browser_required) ? 0 : 1;
  };
  return [...capabilities].sort((left, right) => rank(left) - rank(right));
}

/** Green expresses a public read path or a successful, matching login observation. */
export function openCliPlatformIsGreen(item: Capability, profile: string | null,
  observation: OpenCliAuthObservation | undefined, failed: boolean): boolean {
  return !item.browser_required || (!openCliAccountNeedsLoginCheck(item.platform, item.browser_required) && openCliSearchAccess(item.platform, item.browser_required) === "public_browser" && !!profile && item.registered) || (!failed && !!profile && observation?.profile_ref === profile
    && observation.platform === item.platform && isFreshOpenCliAuthObservation(observation, profile));
}
