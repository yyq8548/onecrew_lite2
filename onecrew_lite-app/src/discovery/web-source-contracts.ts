import { z } from "zod";
import { validateDiscoveryWebNative } from "./web-receipt.ts";
import { discoveryWebProfileTarget } from "./web-profile.ts";
import { NATIVE_READ_FAILURE_CODES, PLATFORM_READ_FAILURE_CODES } from "../platforms/opencli/errors.ts";
const text = z.string().refine(value => value.isWellFormed());
const u32 = z.number().int().min(0).max(4_294_967_295);
export const discoveryWebSourceClaim = z.strictObject({ campaign_id: text, expected_revision: u32.positive(), search_hit_id: text, origin_platform: z.enum(["google", "yahoo", "instagram"]), profile_ref: text, source_url: text, target_platform: z.enum(["youtube", "twitter", "douyin", "instagram", "tiktok", "linkedin", "bilibili", "weibo"]), video_id: text, call_number: u32.positive() });
export type DiscoveryWebSourceClaim = z.infer<typeof discoveryWebSourceClaim>;
export const discoveryWebSourceRead = z.strictObject({ claim: discoveryWebSourceClaim, status: text, native_author_id: text.nullable(), native_material: z.unknown(), stop_code: text.nullable(), captured_at: z.union([z.number().int().safe(), z.bigint()]), candidate_id: text.nullable(), is_primary_source: z.boolean() });
const stops = z.enum(["opencli_timeout", "opencli_output_too_large", "opencli_version_changed", "opencli_daemon_unavailable", "opencli_extension_unavailable", "opencli_profile_unavailable", "opencli_command_unavailable", "opencli_invalid_output", "opencli_invalid_input", "opencli_unavailable", "opencli_command_failed", "opencli_risk_control", "opencli_auth_required", ...NATIVE_READ_FAILURE_CODES, ...PLATFORM_READ_FAILURE_CODES]);
export function discoveryWebSourceTarget(source: string): { platform: "youtube" | "twitter" | "douyin" | "instagram" | "tiktok" | "linkedin" | "bilibili" | "weibo"; id: string; cost: number; handle?: string } | null {
  const url = URL.parse(source);
  if (!url || url.protocol !== "https:" || url.username || url.password || url.port || url.hash) return null;
  if ([...url.searchParams.keys()].every(key => key === "hl") && url.searchParams.getAll("hl").length <= 1 && ["instagram.com", "www.instagram.com"].includes(url.hostname)) {
    const match = /^\/([a-z\d_.]{1,30})\/?$/iu.exec(url.pathname);
    const username = match?.[1]?.toLowerCase();
    if (username && !["accounts", "direct", "explore", "reels", "reel", "p", "stories", "about", "legal", "popular"].includes(username)) return { platform: "instagram", id: username, cost: 1 };
  }
  if ([...url.searchParams.keys()].every(key => key === "lang") && url.searchParams.getAll("lang").length <= 1 && ["tiktok.com", "www.tiktok.com"].includes(url.hostname)) {
    const match = /^\/@([a-z\d_.]{1,24})\/?$/iu.exec(url.pathname);
    if (match) return { platform: "tiktok", id: match[1]!, cost: 1 };
  }
  if (["youtube.com", "www.youtube.com"].includes(url.hostname)) {
    const parts = url.pathname.split("/"), query = url.searchParams.getAll("v");
    const id = url.pathname === "/watch" && query.length === 1 ? query[0] : parts.length === 3 && (parts[1] === "watch" && !url.search || parts[1] === "shorts" || parts[1] === "live") ? parts[2] : null;
    if (id !== null && id !== undefined && /^[A-Za-z0-9_-]{11}$/u.test(id)) return { platform: "youtube", id, cost: 1 };
  }
  const douyin = /^https:\/\/www\.douyin\.com\/video\/([1-9][0-9]{0,19})$/u.exec(source);
  if (douyin) return { platform: "douyin", id: douyin[1]!, cost: 2 };
  const twitter = url.hostname === "x.com" && !url.search ? /^\/([A-Za-z0-9_]{1,15})\/status\/([1-9][0-9]{0,19})$/u.exec(url.pathname) : null;
  return twitter ? { platform: "twitter", id: twitter[2]!, cost: 3, handle: twitter[1]!.toLowerCase() } : discoveryWebProfileTarget(source);
}
export function validateDiscoveryWebSourceResult(rawClaim: DiscoveryWebSourceClaim, material: unknown, stop: string | null): Record<string, unknown> | null {
  const claim = discoveryWebSourceClaim.parse(rawClaim), target = discoveryWebSourceTarget(claim.source_url);
  if (!target || target.platform !== claim.target_platform || target.id !== claim.video_id) throw new Error("web_source_claim_invalid");
  if (material === null) {
    if (claim.target_platform === "youtube" && ["youtube_video_unavailable", "youtube_watch_layout_unsupported", "youtube_author_identity_invalid", "youtube_description_unavailable"].includes(stop ?? "")) return null;
    stops.parse(stop); return null;
  }
  if (stop !== null) throw new Error("web_source_result_invalid");
  return validateDiscoveryWebNative(claim.source_url, claim.target_platform, claim.video_id, material);
}
