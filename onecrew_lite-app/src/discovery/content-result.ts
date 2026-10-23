import { z } from "zod";
import { nativeContentObservations } from "../platforms/opencli/observations.ts";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import { twitterPostContentPlan } from "../platforms/opencli/twitter-posts.ts";
import { validateDiscoveryRetainedContent } from "./native-material.ts";
import { discoveryContentClaim } from "./review-material.ts";
import type { z as Zod } from "zod";
import { discoveryPostIdForSource } from "./post-author-contracts.ts";
import { NATIVE_READ_FAILURE_CODES, PLATFORM_READ_FAILURE_CODES } from "../platforms/opencli/errors.ts";

export type DiscoveryContentClaim = Zod.infer<typeof discoveryContentClaim>;
const text = z.string().refine(value => value.isWellFormed()), stop = z.enum(["opencli_timeout", "opencli_output_too_large", "opencli_version_changed", "opencli_daemon_unavailable", "opencli_extension_unavailable", "opencli_profile_unavailable", "opencli_command_unavailable", "opencli_invalid_output", "opencli_invalid_input", "opencli_unavailable", "opencli_command_failed", "opencli_risk_control", "opencli_auth_required", ...NATIVE_READ_FAILURE_CODES, ...PLATFORM_READ_FAILURE_CODES]);
export function validateDiscoveryContentResult(claim: DiscoveryContentClaim, raw: unknown, failure: string | null): Record<string, unknown> | null {
  discoveryContentClaim.parse(claim);
  if (failure !== null) { stop.parse(failure); if (raw !== null) throw new Error("discovery_content_result_invalid"); return null; }
  const native = z.record(text, z.unknown()).parse(raw), maximum = claim.platform === "hackernews" ? 40000 : ["linkedin", "pixiv", "zhihu"].includes(claim.platform) ? 24000 : 16000;
  if (Buffer.byteLength(stringifyApplicationJson(native)) > maximum) throw new Error("discovery_content_result_too_large");
  if (claim.platform === "instagram") {
    if (!/^https:\/\/www\.instagram\.com\/[a-z0-9_.]{1,30}\/$/u.test(claim.source_url) || !/^[1-9][0-9]{0,24}$/u.test(claim.author_id) && claim.author_id !== claim.source_url) throw new Error("discovery_content_result_invalid");
    const post = z.strictObject({ post_id: text.regex(/^[A-Za-z0-9_-]{1,32}$/u), author_id: z.literal(claim.author_id), source_url: text.regex(/^https:\/\/www\.instagram\.com\/(?:p|reel)\/[A-Za-z0-9_-]{1,32}\/$/u), text: text.refine(value => Buffer.byteLength(value) <= 8000 && !value.includes("\0")), text_truncated: z.boolean(), observations: nativeContentObservations.optional() }).parse(native);
    if (!/^[1-9][0-9]{0,24}$/u.test(post.post_id) && new URL(post.source_url).pathname.split("/")[2] !== post.post_id) throw new Error("discovery_content_result_invalid");
    return post;
  }
  if (claim.platform === "twitter") {
    const post = discoveryPostIdForSource(claim.author_id, claim.source_url);
    const identity = z.object({ account_id: text }).parse(native.identity);
    return twitterPostContentPlan(claim.author_id, post, identity.account_id).decode([native]);
  }
  if (claim.platform === "bilibili") {
    const named = (maximum: number) => text.refine(value => !!value.trim() && [...value].length <= maximum && !/\p{Cc}/u.test(value));
    const value = z.strictObject({ bvid: text, author_id: z.literal(claim.author_id), author: named(200), source_url: z.literal(claim.source_url), title: named(500), description: text.refine(value => Buffer.byteLength(value) <= 8192), description_truncated: z.boolean(), requires_payment: z.boolean(), pay_preview: z.boolean(), payment_status_known: z.boolean().optional(), observations: nativeContentObservations.optional() }).parse(native);
    if (!claim.source_url.startsWith("https://www.bilibili.com/video/") || value.bvid !== claim.source_url.slice("https://www.bilibili.com/video/".length)) throw new Error("discovery_content_result_invalid");
    return value;
  }
  return validateDiscoveryRetainedContent(claim.platform, claim.author_id, claim.source_url, native);
}
