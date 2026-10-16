import { z } from "zod";
const text = z.string().refine(value => value.isWellFormed()), u32 = z.number().int().min(0).max(4294967295);
export const discoveryPostAuthorClaim = z.strictObject({ campaign_id: text, expected_revision: u32, search_hit_id: text, profile_ref: text, author_handle: text, source_url: text, post_id: text, call_number: u32 });
export type DiscoveryPostAuthorClaim = z.infer<typeof discoveryPostAuthorClaim>;
export function discoveryPostIdForSource(handle: string, source: string): string {
  const url = URL.parse(source), parts = url?.pathname.split("/");
  if (!/^[a-z0-9_]{1,15}$/u.test(handle) || !url || url.protocol !== "https:" || url.hostname !== "x.com" || url.port || url.username || url.password || url.search || url.hash || !parts || parts.length !== 4 || parts[1] !== "i" && parts[1] !== handle || parts[2] !== "status" || !/^[1-9][0-9]{0,19}$/u.test(parts[3]!) || source !== `https://x.com/${parts[1]}/status/${parts[3]}`) throw new Error("discovery_post_author_source_invalid");
  return parts[3]!;
}
export const discoveryPostAuthorResult = z.strictObject({ account_id: text.regex(/^[1-9][0-9]{0,19}$/u).nullable(), observed_source_url: text.nullable(), stop_code: z.enum(["opencli_timeout", "opencli_output_too_large", "opencli_version_changed", "opencli_daemon_unavailable", "opencli_extension_unavailable", "opencli_profile_unavailable", "opencli_command_unavailable", "opencli_invalid_output", "opencli_invalid_input", "opencli_unavailable", "opencli_command_failed", "opencli_risk_control", "opencli_auth_required"]).nullable() });
export function validateDiscoveryPostAuthorResult(claim: DiscoveryPostAuthorClaim, raw: unknown) {
  const result = discoveryPostAuthorResult.parse(raw);
  if (result.stop_code === null ? result.account_id === null || discoveryPostIdForSource(claim.author_handle, claim.source_url) !== claim.post_id || result.observed_source_url !== `https://x.com/${claim.author_handle}/status/${claim.post_id}` : result.account_id !== null || result.observed_source_url !== null) throw new Error("discovery_post_author_result_invalid");
  return result;
}
