import { z } from "zod";
import { nativeContentObservations } from "../platforms/opencli/observations.ts";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import { facebookSourceValid } from "../platforms/opencli/facebook-profile.ts";
import { weiboProfilePlan } from "../platforms/opencli/weibo.ts";
import { blueskyPostPlan, blueskyProfilePlan, blueskyUriFromSource } from "../platforms/opencli/bluesky.ts";
import { linkedinContentPlan, linkedinProfilePlan } from "../platforms/opencli/linkedin.ts";
import { validateSavedDiscoveryDouyinContent, validateSavedDiscoveryDouyinProfile } from "./douyin-material.ts";
import { validateHistoricalDiscoveryContent, validateHistoricalDiscoveryProfile } from "./historical-native-material.ts";

const object = z.record(z.string(), z.unknown());
const body = (maximum: number) => z.string().refine(value => value.isWellFormed() && !!value.trim() && Buffer.byteLength(value) <= maximum && !value.includes("\0"));
export function validateDiscoveryRetainedProfile(platform: string, author: string, authorUrl: string, source: string, raw: unknown): Record<string, unknown> {
  const native = object.parse(raw);
  if (Buffer.byteLength(stringifyApplicationJson(native)) > 16000) throw new Error("review_native_profile_binding_unavailable");
  if (["zhihu", "hackernews", "pixiv"].includes(platform)) return validateHistoricalDiscoveryProfile(platform, author, authorUrl, native);
  if (platform === "reddit") {
    z.strictObject({ adapter_version: z.literal("1.0.0"), id: z.literal(author).refine(value => /^t2_[a-z0-9]{1,15}$/u.test(value)), username: z.string().regex(/^[A-Za-z0-9_-]{3,20}$/u), biography: z.string().refine(value => [...value].length <= 2000 && !value.includes("\0")), follower_count: z.number().int().nonnegative().safe().nullable(), profile_url: z.literal(authorUrl), evidence_source: z.literal("native_profile_response") }).refine(value => value.profile_url === `https://www.reddit.com/user/${value.username}/`).parse(native);
  }
  else if (platform === "facebook") {
    const row = z.strictObject({ adapter_version: z.enum(["1.0.0", "2.0.0"]), id: z.literal(author).refine(value => /^[1-9][0-9]{0,19}$/u.test(value)), display_name: z.string().refine(value => !!value.trim() && [...value].length <= 200 && !/\p{Cc}/u.test(value)), follower_count: z.number().int().nonnegative().safe().nullable(), follower_text: z.string().refine(value => [...value].length <= 100 && !/\p{Cc}/u.test(value)), profile_url: z.literal(`https://www.facebook.com/profile.php?id=${author}`), evidence_source: z.string(), contact_links: z.array(z.string()).optional(), observed_profile_url: z.string().optional() }).parse(native);
    if (row.adapter_version === "1.0.0") {
      if (Object.hasOwn(native, "observed_profile_url") || row.evidence_source !== "visible_numeric_profile_header" || row.contact_links !== undefined && (row.contact_links.length > 1 || row.contact_links.some(link => link !== `https://www.facebook.com/profile.php?id=${author}&sk=about_contact_and_basic_info`))) throw new Error("review_native_profile_binding_unavailable");
    } else {
      const base = row.observed_profile_url;
      if (base === undefined || !facebookSourceValid(base) || base.endsWith("/") || row.evidence_source !== "native_profile_header_response" || row.contact_links === undefined || row.contact_links.length > 2 || new Set(row.contact_links).size !== row.contact_links.length || row.contact_links.some(link => ![`${base}/directory_contact_info`, `${base}/directory_links`].includes(link)) || base.toLowerCase() !== source.replace(/\/+$/u, "").toLowerCase()) throw new Error("review_native_profile_binding_unavailable");
    }
    if (row.follower_count !== null) {
      const rawCount = row.follower_text.trim().toLowerCase().replace(/ followers?$/u, "");
      if (!/^(?:0|[1-9][0-9]*|[1-9][0-9]{0,2}(?:,[0-9]{3})+)$/u.test(rawCount) || !/ followers?$/u.test(row.follower_text.trim().toLowerCase()) || BigInt(rawCount.replaceAll(",", "")) !== BigInt(row.follower_count)) throw new Error("review_native_profile_binding_unavailable");
    }
  }
  else if (platform === "weibo") weiboProfilePlan(author).decode([native]);
  else if (platform === "bluesky") blueskyProfilePlan(author).decode([native]);
  else if (platform === "linkedin") linkedinProfilePlan(author).decode([native]);
  else if (platform === "douyin") { const saved = validateSavedDiscoveryDouyinProfile(native); if (saved.author_id !== author) throw new Error("review_native_profile_binding_unavailable"); }
  else throw new Error("review_platform_unsupported");
  if (native.profile_url !== authorUrl) throw new Error("review_native_profile_binding_unavailable"); return native;
}

export function validateDiscoveryRetainedContent(platform: string, author: string, source: string, raw: unknown): Record<string, unknown> {
  const observed = object.parse(raw), observations = observed.observations === undefined ? undefined : nativeContentObservations.parse(observed.observations);
  const native = { ...observed }; delete native.observations;
  if (Buffer.byteLength(stringifyApplicationJson(observed)) > (platform === "hackernews" ? 40000 : ["linkedin", "zhihu", "pixiv"].includes(platform) ? 24000 : 16000)) throw new Error("review_native_content_binding_unavailable");
  if (["zhihu", "hackernews", "pixiv"].includes(platform)) return validateHistoricalDiscoveryContent(platform, author, source, native);
  if (platform === "linkedin") linkedinContentPlan(author, z.string().parse(native.account_id)).decode([native]);
  else if (platform === "douyin") validateSavedDiscoveryDouyinContent(author, source, native);
  else if (platform === "bluesky") blueskyPostPlan(blueskyUriFromSource(source)).decode([native]);
  else if (platform === "reddit" || platform === "facebook") {
    const id = platform === "reddit" ? /^https:\/\/www\.reddit\.com\/comments\/([a-z0-9]{1,15})\/$/u.exec(source)?.[1] : /^https:\/\/www\.facebook\.com\/reel\/([1-9][0-9]{0,24})$/u.exec(source)?.[1];
    if (!id || !(platform === "reddit" ? /^t2_[a-z0-9]{1,15}$/u : /^[1-9][0-9]{0,19}$/u).test(author)) throw new Error("review_native_content_binding_unavailable");
    z.strictObject({ post_id: z.literal(id), author_id: z.literal(author), source_url: z.literal(source), text: body(8000), text_truncated: z.boolean() }).parse(native);
  } else if (platform === "weibo") {
    const match = /^https:\/\/weibo\.com\/([1-9][0-9]{0,19})\/([A-Za-z0-9]{1,20})$/u.exec(source);
    if (!match || match[1] !== author) throw new Error("review_native_content_binding_unavailable");
    z.strictObject({ post_id: z.string().regex(/^[1-9][0-9]{0,19}$/u), author_id: z.literal(author), author: z.string().refine(value => !!value.trim() && [...value].length <= 200 && !/\p{Cc}/u.test(value)), source_url: z.literal(source), text: z.string().refine(value => Buffer.byteLength(value) <= 16000 && !value.includes("\0")), text_truncated: z.boolean(), text_completeness_known: z.boolean(), is_repost: z.literal(false), observations: nativeContentObservations.optional() }).parse(native);
  } else throw new Error("review_platform_unsupported");
  return { ...native, ...(observations ? { observations } : {}) };
}
