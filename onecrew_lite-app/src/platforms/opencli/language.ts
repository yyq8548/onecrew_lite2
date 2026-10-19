import { z } from "zod";
import { nativeContentObservations } from "./observations.ts";
import { OpenCliError } from "./errors.ts";
import type { ReviewedOpenCliRead } from "./read-plan.ts";
import { nativeCommentsPlan } from "./native-comments.ts";

const text = z.string().refine(value => value.isWellFormed());
const invalid = (): never => { throw new OpenCliError("opencli_invalid_output"); };
const input = (): never => { throw new OpenCliError("opencli_invalid_input"); };
const numeric = (value: string, maximum: number) => value.length <= maximum && /^[1-9]\d*$/u.test(value);
function rows(value: unknown, maximum: number, minimum = 0) { const result = z.array(z.record(z.string(), z.unknown())).min(minimum).max(maximum).safeParse(value); return result.success ? result.data : invalid(); }
function excerpt(value: unknown, maximum: number): string {
  const parsed = text.safeParse(value);
  if (!parsed.success) throw new OpenCliError("opencli_invalid_output");
  if (parsed.data.includes("\0")) invalid();
  let output = ""; let bytes = 0;
  for (const character of parsed.data) { const length = Buffer.byteLength(character); if (bytes + length > maximum) break; output += character; bytes += length; }
  return output;
}
function tiktokTarget(username: string, authorId: string) { if (!/^[A-Za-z\d_.]{1,24}$/u.test(username) || !numeric(authorId, 25)) input(); }
export function tiktokRecentLanguagePlan(username: string, authorId: string) {
  tiktokTarget(username, authorId);
  return { site: "onecrew-comments-tiktok", name: "recent", args: ["--author-id", authorId, "-f", "json", "--", username], addons: ["onecrew-browser-search", "onecrew-tiktok-profile"], maximumBytes: 8192, decode(value: unknown) {
    const seen = new Set<string>();
    return rows(value, 3, 1).map(row => {
      const source = typeof row.source_url === "string" ? row.source_url : invalid();
      const prefix = `https://www.tiktok.com/@${username}/video/`;
      if (Object.keys(row).length !== 1 || !source.startsWith(prefix) || !numeric(source.slice(prefix.length), 25) || seen.has(source)) invalid();
      seen.add(source); return source;
    });
  } } satisfies ReviewedOpenCliRead<unknown>;
}
export function tiktokLanguagePlan(username: string, authorId: string, source: string, includeComments = true) {
  tiktokTarget(username, authorId);
  const prefix = `https://www.tiktok.com/@${username}/video/`;
  if (!source.startsWith(prefix) || !numeric(source.slice(prefix.length), 25)) input();
  const plan = nativeCommentsPlan({ kind: "tiktok_video", source }, includeComments ? 5 : 0);
  return { ...plan, decode(value: unknown) {
    const items = rows(value, 21, 1);
    if (!includeComments && items.length !== 1) invalid();
    const post = items[0]!;
    if (post.type !== "POST" || post.source_url !== source || post.native_author_id !== authorId || post.author !== username) invalid();
    const body = excerpt(post.text, 8000);
    const seen = new Set<string>();
    const comments: { source_url: string; text: string }[] = [];
    for (const row of items.slice(1)) {
      if (row.type !== "COMMENT" || row.source_url !== source) invalid();
      const body = excerpt(row.text, 2000);
      if (!body.trim()) invalid();
      if (!seen.has(body)) { seen.add(body); comments.push({ source_url: source, text: body }); }
    }
    if (!body.trim() && !comments.length) invalid();
    return { author_id: authorId, source_url: source, text: body, comments, ...(post.observations ? { observations: nativeContentObservations.parse(post.observations) } : {}) };
  } } satisfies ReviewedOpenCliRead<unknown>;
}
export function bilibiliLanguagePlan(mid: string, source: string | null) {
  if (!numeric(mid, 20)) input();
  const id = source === null ? "recent" : /^https:\/\/www\.bilibili\.com\/video\/(BV[A-Za-z\d]{10})$/u.exec(source)?.[1] ?? input();
  return { site: "onecrew-bilibili-video", name: "language", args: ["-f", "json", "--", mid, id], addons: ["onecrew-bilibili-video", "onecrew-browser-search"], maximumBytes: 64000, decode(value: unknown) {
    if (source !== null) {
      const row = rows(value, 1, 1)[0]!;
      if (row.author_id !== mid || row.source_url !== source) invalid();
      return row;
    }
    const seen = new Set<string>();
    return rows(value, 3).map(row => {
      const url = typeof row.source_url === "string" ? row.source_url : invalid();
      if (Object.keys(row).length !== 1 || !/^https:\/\/www\.bilibili\.com\/video\/BV[A-Za-z\d]{10}$/u.test(url) || seen.has(url)) invalid();
      seen.add(url); return url;
    });
  } } satisfies ReviewedOpenCliRead<unknown>;
}
export function instagramRecentOriginalsPlan(username: string, authorId: string, query = "", sourceUrl?: string) {
  if (!/^[a-z\d_.]{1,30}$/u.test(username) || !numeric(authorId, 25)) input();
  if (Buffer.byteLength(query) > 2000 || /[\p{Cc}&&[^\n\r\t]]/v.test(query)) input();
  if (sourceUrl && !new RegExp(`^https://www\\.instagram\\.com/${username.replaceAll(".", "\\.")}/(?:p|reel)/[A-Za-z\\d_-]{1,32}/$`, "u").test(sourceUrl)) input();
  const row = z.object({ post_id: text.regex(/^[A-Za-z\d_-]{1,32}$/u), author_id: z.literal(authorId), source_url: text.regex(/^https:\/\/www\.instagram\.com\/(?:p|reel)\/[A-Za-z\d_-]{1,32}\/$/u), text: text.refine(value => Buffer.byteLength(value) <= 8000 && !value.includes("\0")), text_truncated: z.boolean(), observations: nativeContentObservations.optional() }).strict();
  return { site: "onecrew-instagram-profile", name: "recent", args: ["-f", "json", "--", username, authorId, ...(query || sourceUrl ? [query] : []), ...(sourceUrl ? [sourceUrl] : [])], addons: ["onecrew-browser-search"], maximumBytes: 30000, decode(value: unknown) {
    const parsed = z.array(row).max(3).safeParse(value);
    if (!parsed.success || parsed.data.some(item => !numeric(item.post_id, 25) && new URL(item.source_url).pathname.split("/")[2] !== item.post_id) || new Set(parsed.data.map(item => item.post_id)).size !== parsed.data.length || new Set(parsed.data.map(item => item.source_url)).size !== parsed.data.length) invalid();
    if (!parsed.success) throw new OpenCliError("opencli_invalid_output");
    if (sourceUrl && (parsed.data.length !== 1 || new URL(sourceUrl).pathname.split("/").slice(-3).join("/") !== new URL(parsed.data[0]!.source_url).pathname.slice(1))) invalid();
    return parsed.data;
  } } satisfies ReviewedOpenCliRead<unknown>;
}
export function instagramCommentLanguagePlan(username: string, source: string) {
  const code = /^https:\/\/www\.instagram\.com\/(?:p|reel)\/([A-Za-z\d_-]{1,32})\/$/u.exec(source)?.[1] ?? input();
  const plan = nativeCommentsPlan({ kind: "instagram_post", source }, 5);
  return { ...plan, decode(value: unknown) {
    const items = rows(value, 6, 1);
    const post = items[0]!;
    if (post.type !== "POST" || post.source_url !== source || post.id !== code || post.author !== username || post.author_url !== `https://www.instagram.com/${username}/` || post.identity_source !== "visible_profile_link") invalid();
    const prefix = `https://www.instagram.com/p/${code}/c/`;
    const seen = new Set<string>();
    const comments = items.slice(1).map(row => {
      const url = typeof row.source_url === "string" ? row.source_url : invalid();
      const id = url.startsWith(prefix) && url.endsWith("/") ? url.slice(prefix.length, -1) : "";
      const body = text.safeParse(row.text);
      if (!numeric(id, 25) || row.type !== "COMMENT" || row.id !== id || row.text_truncated !== false || !body.success || !body.data.trim() || Buffer.byteLength(body.data) > 2000 || body.data.includes("\0") || seen.has(url)) invalid();
      seen.add(url); return { source_url: url, text: body.data };
    });
    return { source_url: source, comments };
  } } satisfies ReviewedOpenCliRead<unknown>;
}
