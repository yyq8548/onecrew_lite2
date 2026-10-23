import { z } from "zod";
import { discoveryLanguageSample } from "./review-material.ts";
import { blueskyPostTarget, validBlueskyDid } from "../platforms/opencli/bluesky.ts";

const text = z.string().refine(value => value.isWellFormed());
const u32 = z.number().int().min(0).max(4_294_967_295);
export const discoveryLanguageClaim = z.strictObject({
  campaign_id: text, revision: u32, search_hit_id: text, candidate_id: text,
  platform: text, profile_ref: text, author_id: text, author_handle: text, root_url: text,
  ordinal: u32, call_number: u32, previous_urls: z.array(text), saved_content_urls: z.array(text),
});
export type DiscoveryLanguageClaim = z.infer<typeof discoveryLanguageClaim>;
export type DiscoveryLanguageSample = z.infer<typeof discoveryLanguageSample>;
const numeric = (value: string, limit = 20, positive = true) => new RegExp(`^${positive ? "[1-9]" : "[0-9]"}[0-9]{0,${limit - 1}}$`, "u").test(value);
const bounded = (value: string, maximum: number, nonempty = false) => value.isWellFormed() && !value.includes("\0") && Buffer.byteLength(value) <= maximum && (!nonempty || !!value.trim());
const unique = (values: readonly string[]) => new Set(values).size === values.length;
function twitterSource(source: string, handle?: string): boolean {
  if (source.length > 2048 || !/^https:\/\/x\.com\/[A-Za-z0-9_]{1,15}\/status\/[0-9]{1,20}$/u.test(source)) return false;
  const url = new URL(source);
  return handle === undefined || url.pathname.split("/")[1] === handle;
}
function hackernewsId(source: string): string | null {
  if (!/^https:\/\/news\.ycombinator\.com\/item\?id=[1-9][0-9]{0,19}$/u.test(source)) return null;
  const id = new URL(source).searchParams.get("id")!;
  return BigInt(id) <= 9_007_199_254_740_991n ? id : null;
}
function blueskyUri(source: string): string | null {
  const match = /^https:\/\/bsky\.app\/profile\/(.+)\/post\/([A-Za-z0-9_-]{1,64})$/u.exec(source);
  if (!match) return null;
  const uri = `at://${match[1]}/app.bsky.feed.post/${match[2]}`;
  return blueskyPostTarget(uri)?.source === source ? uri : null;
}

// 与原 DiscoveryLanguageSample::valid_for 一致：只接受匹配原文的首个有界评论窗口。
export function validDiscoveryLanguageSample(raw: unknown, claim: DiscoveryLanguageClaim): raw is DiscoveryLanguageSample {
  const parsed = discoveryLanguageSample.safeParse(raw);
  if (!parsed.success) return false;
  const sample = parsed.data, comments = sample.comments;
  if (claim.ordinal !== 1 || claim.previous_urls.length !== 0 || sample.source_url !== claim.root_url || comments.length > 5 || sample.author_id !== claim.author_id) return false;
  let platform = claim.platform;
  if (platform === "google" || platform === "yahoo") platform = /^https:\/\/www\.douyin\.com\/video\/[1-9][0-9]{0,19}$/u.test(claim.root_url) ? "douyin" : twitterSource(claim.root_url) ? "twitter" : "youtube";
  const prefix = `${sample.source_url}#comment-`;
  const anchors = (maximum: number, nonempty: boolean) => unique(comments.map(comment => comment.source_url)) && comments.every(comment => comment.source_url.startsWith(prefix) && numeric(comment.source_url.slice(prefix.length), maximum) && bounded(comment.text, 2000, nonempty));
  if (platform === "douyin") return /^MS4wLjABAAAA[A-Za-z0-9_-]{8,88}$/u.test(sample.author_id) && claim.author_handle === claim.author_id && /^https:\/\/www\.douyin\.com\/video\/[1-9][0-9]{0,19}$/u.test(sample.source_url) && bounded(sample.text, 12000, true) && anchors(20, false);
  if (platform === "linkedin") return /^ACo[A-Za-z0-9_-]{16,125}$/u.test(sample.author_id) && /^[A-Za-z0-9][A-Za-z0-9_-]{0,99}$/u.test(claim.author_handle) && !["me", "self"].includes(claim.author_handle.toLowerCase()) && /^https:\/\/www\.linkedin\.com\/feed\/update\/urn:li:activity:[1-9][0-9]{0,19}\/$/u.test(sample.source_url) && bounded(sample.text, 16000, true) && anchors(20, false);
  if (platform === "hackernews") {
    const root = hackernewsId(sample.source_url), ids = comments.map(comment => hackernewsId(comment.source_url));
    return root !== null && /^[A-Za-z0-9_-]{1,64}$/u.test(sample.author_id) && bounded(sample.text, 16000, true) && unique(ids.filter((id): id is string => id !== null)) && comments.every((comment, index) => ids[index] !== null && ids[index] !== root && bounded(comment.text, 2000, true));
  }
  if (platform === "zhihu") return /^[a-f0-9]{32}$/u.test(sample.author_id) && /^https:\/\/www\.zhihu\.com\/question\/[1-9][0-9]{0,19}\/answer\/[1-9][0-9]{0,19}$/u.test(sample.source_url) && bounded(sample.text, 16000) && anchors(20, false);
  if (platform === "bluesky") {
    if (!validBlueskyDid(sample.author_id) || !sample.source_url.startsWith(`https://bsky.app/profile/${sample.author_id}/post/`) || blueskyUri(sample.source_url) === null || !bounded(sample.text, 16000) || !sample.text.trim() && comments.length === 0) return false;
    return unique(comments.map(comment => comment.source_url)) && comments.every(comment => {
      if (!comment.source_url.startsWith(prefix) || !bounded(comment.text, 2000, true)) return false;
      const uri = comment.source_url.slice(prefix.length), match = /^at:\/\/(.+)\/app\.bsky\.feed\.post\/([^/]+)$/u.exec(uri);
      if (!match) return false;
      const source = `https://bsky.app/profile/${match[1]}/post/${match[2]}`;
      return source !== sample.source_url && blueskyUri(source) === uri;
    });
  }
  if (platform === "weibo") {
    const source = /^https:\/\/weibo\.com\/([1-9][0-9]{0,19})\/[A-Za-z0-9]{1,20}$/u.exec(sample.source_url);
    return numeric(sample.author_id) && source?.[1] === sample.author_id && bounded(sample.text, 16000) && (!!sample.text.trim() || comments.length > 0) && anchors(20, true);
  }
  if (platform === "instagram") {
    const source = /^https:\/\/www\.instagram\.com\/(?:p|reel)\/([A-Za-z0-9_-]{1,32})\/$/u.exec(sample.source_url);
    const commentPrefix = source ? `https://www.instagram.com/p/${source[1]}/c/` : "";
    return numeric(sample.author_id, 25) && source !== null && bounded(sample.text, 8000) && (!!sample.text.trim() || comments.length > 0) && unique(comments.map(comment => comment.source_url)) && comments.every(comment => bounded(comment.text, 2000, true) && comment.source_url.startsWith(commentPrefix) && comment.source_url.endsWith("/") && numeric(comment.source_url.slice(commentPrefix.length, -1), 25));
  }
  if (platform === "facebook") {
    const commentPrefix = `${sample.source_url}?comment_id=`;
    return /^https:\/\/www\.facebook\.com\/reel\/[1-9][0-9]{0,24}$/u.test(sample.source_url) && claim.saved_content_urls.includes(sample.source_url) && bounded(sample.text, 8000) && (!!sample.text.trim() || comments.length > 0) && unique(comments.map(comment => comment.source_url)) && comments.every(comment => bounded(comment.text, 2000, true) && comment.source_url.startsWith(commentPrefix) && numeric(comment.source_url.slice(commentPrefix.length), 25));
  }
  if (platform === "tiktok") return numeric(sample.author_id, 25) && /^[A-Za-z0-9_.]{1,24}$/u.test(claim.author_handle) && sample.source_url.startsWith(`https://www.tiktok.com/@${claim.author_handle}/video/`) && numeric(sample.source_url.slice(`https://www.tiktok.com/@${claim.author_handle}/video/`.length), 25) && bounded(sample.text, 8000) && (!!sample.text.trim() || comments.length > 0) && unique(comments.map(comment => comment.text)) && comments.every(comment => comment.source_url === sample.source_url && bounded(comment.text, 2000, true));
  if (platform === "bilibili") {
    const commentPrefix = `${sample.source_url}#reply`;
    return numeric(sample.author_id) && /^https:\/\/www\.bilibili\.com\/video\/BV[A-Za-z0-9]{10}$/u.test(sample.source_url) && bounded(sample.text, 8000) && (!!sample.text.trim() || comments.length > 0) && unique(comments.map(comment => comment.source_url === sample.source_url ? comment.text : comment.source_url)) && comments.every(comment => bounded(comment.text, 2000, true) && (comment.source_url === sample.source_url || comment.source_url.startsWith(commentPrefix) && numeric(comment.source_url.slice(commentPrefix.length))));
  }
  if (platform === "youtube") {
    const video = /^https:\/\/www\.youtube\.com\/watch\?v=([A-Za-z0-9_-]{11})$/u.exec(sample.source_url), commentPrefix = video ? `https://www.youtube.com/watch?v=${video[1]}&lc=` : "";
    return /^UC[A-Za-z0-9_-]{22}$/u.test(sample.author_id) && video !== null && bounded(sample.text, 8000) && (!!sample.text.trim() || comments.length > 0) && unique(comments.map(comment => comment.source_url)) && comments.every(comment => bounded(comment.text, 2000, true) && comment.source_url.startsWith(commentPrefix) && /^[A-Za-z0-9_.-]{1,200}$/u.test(comment.source_url.slice(commentPrefix.length)));
  }
  return platform === "twitter" && numeric(sample.author_id, 20, false) && bounded(sample.text, 8000, true) && twitterSource(sample.source_url, claim.author_handle) && unique(comments.map(comment => comment.source_url)) && comments.every(comment => bounded(comment.text, 2000, true) && comment.source_url !== sample.source_url && twitterSource(comment.source_url));
}
