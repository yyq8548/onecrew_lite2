import type { DatabaseSync } from "node:sqlite";
import { isDeepStrictEqual } from "node:util";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { youtubeVideoEvidencePlan } from "../platforms/opencli/youtube-evidence.ts";
import { twitterPostContentPlan, twitterPostIdentityPlan } from "../platforms/opencli/twitter-posts.ts";
import { douyinContentPlan, douyinProfilePlan } from "../platforms/opencli/douyin.ts";
import { instagramProfilePlan, tiktokProfilePlan } from "../platforms/opencli/profiles.ts";
import { instagramProfileIdentityPlan } from "../platforms/opencli/profile-identity.ts";
import { discoveryWebProfileTarget, validateDiscoveryWebProfile } from "./web-profile.ts";
const object = z.record(z.string(), z.unknown());

export function validateDiscoveryWebNative(source: string, target: string, video: string, raw: unknown): Record<string, unknown> {
  const native = object.parse(raw), url = URL.parse(source);
  if (!url || url.protocol !== "https:" || url.username || url.password || url.port || url.hash || Buffer.byteLength(stringifyApplicationJson(native)) > 65536) throw new Error("web_source_native_material_invalid");
  const profileTarget = discoveryWebProfileTarget(source);
  if (profileTarget) {
    if (profileTarget.platform !== target || profileTarget.id !== video) throw new Error("web_source_claim_invalid");
    const pair = z.strictObject({ identity: object, profile: object }).parse(native), checked = validateDiscoveryWebProfile(source, pair.profile);
    if (!isDeepStrictEqual(pair, checked)) throw new Error("web_source_native_material_invalid");
    return checked;
  }
  if (target === "instagram" || target === "tiktok") {
    const instagram = target === "instagram";
    const username = (instagram ? /^\/([a-z\d_.]{1,30})\/?$/iu : /^\/@([a-z\d_.]{1,24})\/?$/iu).exec(url.pathname)?.[1];
    const languageKey = instagram ? "hl" : "lang";
    if ([...url.searchParams.keys()].some(key => key !== languageKey) || url.searchParams.getAll(languageKey).length > 1 || ![`${target}.com`, `www.${target}.com`].includes(url.hostname) || (instagram ? username?.toLowerCase() : username) !== video) throw new Error("web_source_claim_invalid");
    const pair = z.strictObject({ identity: object, profile: object }).parse(native);
    if (instagram) {
      instagramProfileIdentityPlan(video).decode([pair.identity]);
      instagramProfilePlan(video, `https://www.instagram.com/${video}/`).decode([pair.profile]);
      if (!isDeepStrictEqual(pair.identity, pair.profile)) throw new Error("web_source_native_material_invalid");
    } else {
      const author = z.string().regex(/^[1-9][0-9]{0,24}$/u).parse(pair.identity.id);
      tiktokProfilePlan(video, author).decode([pair.identity]);
      tiktokProfilePlan(video, author).decode([pair.profile]);
      if (!isDeepStrictEqual(pair.identity, pair.profile)) throw new Error("web_source_native_material_invalid");
    }
    return native;
  }
  if (target === "youtube") {
    if (!["youtube.com", "www.youtube.com"].includes(url.hostname)) throw new Error("web_source_claim_invalid");
    const parts = url.pathname.split("/"), id = url.pathname === "/watch" && url.searchParams.getAll("v").length === 1 ? url.searchParams.get("v") : parts.length === 3 && (parts[1] === "watch" && !url.search || ["shorts", "live"].includes(parts[1]!)) ? parts[2] : null;
    if (id !== video || !/^[A-Za-z0-9_-]{11}$/u.test(video)) throw new Error("web_source_claim_invalid");
    youtubeVideoEvidencePlan(video).decode([native]); return native;
  }
  if (target === "douyin") {
    if (source !== `https://www.douyin.com/video/${video}` || !/^[1-9][0-9]{0,24}$/u.test(video)) throw new Error("web_source_claim_invalid");
    const pair = z.strictObject({ profile: object, content: object }).parse(native), author = z.string().parse(pair.content.author_id);
    douyinProfilePlan(author).decode([pair.profile]); douyinContentPlan(video, author).decode([pair.content]);
    if (pair.content.text_truncated !== false || pair.profile.author_id !== author) throw new Error("web_source_native_material_invalid"); return native;
  }
  if (target !== "twitter" || url.hostname !== "x.com" || url.search) throw new Error("web_source_claim_invalid");
  const match = /^\/([A-Za-z0-9_]{1,15})\/status\/([1-9][0-9]{0,19})$/u.exec(url.pathname);
  if (!match || match[2] !== video) throw new Error("web_source_claim_invalid");
  const handle = match[1]!.toLowerCase(), parts = z.strictObject({ identity: object, profile: object, content: object }).parse(native);
  twitterPostIdentityPlan(handle, video).decode([parts.identity]);
  const account = z.string().parse(parts.identity.account_id);
  twitterPostContentPlan(handle, video, account).decode([parts.content]);
  const bounded = (maximum: number, multiline = false) => z.string().refine(value => Buffer.byteLength(value) <= maximum && !(multiline ? /[\p{Cc}&&[^\n]]/v : /\p{Cc}/u).test(value));
  z.strictObject({ adapter_version: z.literal("1.0.0"), account_id: z.literal(account), handle: z.literal(handle), profile_url: z.literal(`https://x.com/${handle}`), name: bounded(800).refine(value => !!value.trim()), bio: bounded(8000, true), self_declared_location: bounded(800), follower_count: z.number().int().nonnegative().safe().nullable(), identity_kind: z.literal("native_id"), evidence_source: z.literal("x_user_by_screen_name") }).parse(parts.profile);
  if (!isDeepStrictEqual(parts.content.identity, parts.identity)) throw new Error("web_source_native_material_invalid"); return native;
}

export function loadDiscoveryWebSourceReceipt(database: DatabaseSync, campaign: string, hit: string) {
  if (!database.isTransaction) throw new Error("web_source_requires_snapshot");
  const raw = database.prepare(`SELECT p.expected_revision,p.origin_platform,p.profile_ref,p.source_url,p.target_platform,p.video_id,p.call_number,
    p.status,p.native_author_id,p.native_material_json,p.stop_code,p.updated_at,b.candidate_id,
    COALESCE(c.source_platform=p.origin_platform AND c.source_profile_ref=p.profile_ref AND c.criteria_revision=p.expected_revision
      AND json_extract(c.evidence_json,'$.source_urls[0]')=p.source_url,0) AS is_primary_source
    FROM discovery_web_source_reads p
    JOIN discovery_search_hits h ON h.id=p.search_hit_id AND h.discovery_campaign_id=p.discovery_campaign_id
      AND h.platform=p.origin_platform AND h.profile_ref=p.profile_ref AND h.source_url=p.source_url
    LEFT JOIN discovery_video_identity_bindings b ON b.search_hit_id=p.search_hit_id AND b.discovery_campaign_id=p.discovery_campaign_id
      AND b.criteria_revision=p.expected_revision AND b.native_author_id=p.native_author_id
    LEFT JOIN discovery_candidates c ON c.id=b.candidate_id AND c.discovery_campaign_id=p.discovery_campaign_id
      AND c.platform=p.target_platform AND c.platform_account_id=p.native_author_id
    WHERE p.discovery_campaign_id=?1 AND p.search_hit_id=?2`).get(campaign, hit);
  if (!raw) throw new Error("web_source_receipt_not_found");
  const row = z.object({ expected_revision: z.number().int().positive(), origin_platform: z.enum(["google", "yahoo", "instagram"]), profile_ref: z.string(), source_url: z.string(), target_platform: z.enum(["youtube", "twitter", "douyin", "instagram", "tiktok", "linkedin", "bilibili", "weibo"]), video_id: z.string(), call_number: z.number().int().positive(), status: z.string(), native_author_id: z.string().nullable(), native_material_json: z.string().nullable(), stop_code: z.string().nullable(), updated_at: z.number().int().safe(), candidate_id: z.string().nullable(), is_primary_source: z.union([z.literal(0), z.literal(1)]) }).parse(raw);
  if (row.stop_code !== null || row.native_material_json === null) throw new Error("web_source_native_material_unavailable");
  return { ...row, native: validateDiscoveryWebNative(row.source_url, row.target_platform, row.video_id, parseApplicationJson(Buffer.from(row.native_material_json))) };
}
