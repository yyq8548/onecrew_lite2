import { isIP } from "node:net";
import { z } from "zod";
import { creatorSearchSource } from "./sources.ts";
import { OpenCliError } from "./errors.ts";

export interface CreatorSearchHit { source_url: string; title: string; author_label: string; author_id: string | null; author_url: string | null }
const text = z.string().refine(value => value.isWellFormed());
const clean = (value: string, maximum: number) => [...value.split(/\p{White_Space}+/u).filter(Boolean).join(" ")].slice(0, maximum).join("");
const id = (value: string, maximum = 20) => /^[1-9]\d*$/u.test(value) && value.length <= maximum;
const invalid = (): never => { throw new OpenCliError("opencli_invalid_output"); };
function parse<T>(schema: z.ZodType<T>, value: unknown): T { const result = schema.safeParse(value); return result.success ? result.data : invalid(); }

export const tiktokSearchVideoSchema = z.object({ adapter_version: z.literal("1.0.0"), id: text.refine(value => id(value, 25)), author_id: text.refine(value => id(value, 25)), author: text.regex(/^[A-Za-z\d_.]{1,24}$/u), desc: text.refine(value => value.trim().length > 0 && Buffer.byteLength(value) <= 16000 && !/[\p{Cc}&&[^\n\r\t]]/v.test(value)), url: text }).strict().refine(row => row.url === `https://www.tiktok.com/@${row.author}/video/${row.id}`);
export const bilibiliSearchVideoSchema = z.object({ adapter_version: z.literal("1.0.0"), bvid: text.regex(/^BV[A-Za-z\d]{10}$/u), author_id: text.refine(value => id(value)), author: text.refine(value => !!value.trim() && [...value].length <= 200 && !/\p{Cc}/u.test(value)), title: text.refine(value => !!value.trim() && [...value].length <= 500 && !/\p{Cc}/u.test(value)), url: text, author_url: text }).strict().refine(row => row.url === `https://www.bilibili.com/video/${row.bvid}` && row.author_url === `https://space.bilibili.com/${row.author_id}`);

function pinterest(row: unknown): CreatorSearchHit {
  const fields = { author_id: text.refine(value => id(value)), username: text.regex(/^[A-Za-z\d_]{1,30}$/u), author_label: text.refine(value => !!value.trim() && Buffer.byteLength(value) <= 512 && !/\p{Cc}/u.test(value)), author_url: text, source_url: text, title: text };
  if (typeof row === "object" && row !== null && "provider" in row && row.provider === "onecrew-pinterest-pin-search-v1") {
    const value = parse(z.object({ ...fields, provider: z.literal("onecrew-pinterest-pin-search-v1"), pin_id: text.refine(value => id(value)) }).strict(), row);
    if (value.author_url !== `https://www.pinterest.com/${value.username}/` || value.source_url !== `https://www.pinterest.com/pin/${value.pin_id}/` || !value.title.trim() || [...value.title].length > 500 || /[\p{Cc}&&[^\p{White_Space}]]/v.test(value.title)) invalid();
    return { source_url: value.source_url, title: clean(value.title, 500), author_label: value.author_label, author_id: value.author_id, author_url: value.author_url };
  }
  const value = parse(z.object(fields).strict(), row);
  const expected = `https://www.pinterest.com/${value.username}/`;
  if (value.source_url !== expected || value.author_url !== expected || value.title !== value.author_label) invalid();
  return { source_url: expected, title: value.title, author_label: value.author_label, author_id: value.author_id, author_url: expected };
}

function generic(platform: string, input: unknown): CreatorSearchHit {
  const row = parse(z.record(z.string(), z.unknown()), input);
  const field = (key: string): string => typeof row[key] === "string" ? parse(text, row[key]) : "";
  let source = platform === "linkedin" ? field("profile_url") : field("url");
  let title = platform === "twitter" ? field("text") : platform === "instagram" ? field("name") || field("username") : platform === "douyin" ? field("desc") : platform === "linkedin" ? field("name") : field("title");
  const author = platform === "youtube" ? field("channel") : platform === "instagram" ? field("username") : platform === "facebook" ? field("text") : platform === "linkedin" ? field("name") : ["google", "yahoo"].includes(platform) ? "" : field("author");
  const candidate = z.url().safeParse(source);
  if (!candidate.success || Buffer.byteLength(source) > 2048) invalid();
  const url = new URL(source);
  const hosts: Readonly<Record<string, readonly string[]>> = { youtube: ["youtube.com", "www.youtube.com"], twitter: ["x.com", "www.x.com", "twitter.com", "www.twitter.com"], instagram: ["instagram.com", "www.instagram.com"], facebook: ["facebook.com", "www.facebook.com", "m.facebook.com"], linkedin: ["linkedin.com", "www.linkedin.com"], reddit: ["reddit.com", "www.reddit.com"], douyin: ["douyin.com", "www.douyin.com"], weibo: ["weibo.com", "www.weibo.com", "s.weibo.com"], tieba: ["tieba.baidu.com"] };
  const host = url.hostname;
  const webHost = ["google", "yahoo"].includes(platform) && host.includes(".") && ![".localhost", ".local", ".internal", ".test"].some(suffix => host.endsWith(suffix)) && isIP(host.replace(/^\[|\]$/gu, "")) === 0;
  if (url.protocol !== "https:" || url.username || url.password || url.port || !(hosts[platform]?.includes(host) || webHost)) invalid();
  if (platform === "google" && ["google.com", "www.google.com"].includes(host) && ["/goto", "/aclk", "/url"].includes(url.pathname)) invalid();
  if (platform === "linkedin" && field("headline").trim()) title = `${title}\n${field("headline")}`;
  title = clean(title, 500);
  if (!title) invalid();
  let authorId: string | null = null;
  let authorUrl: string | null = null;
  if (platform === "twitter") {
    const post = field("id");
    const handle = field("author").replace(/^@/u, "");
    const native = url.pathname.match(/^\/([^/]+)\/status\/([^/]+)$/u);
    if (!id(post) || !/^[A-Za-z\d_]{1,15}$/u.test(handle) || !(url.pathname === `/i/status/${post}` || native && native[1]!.toLowerCase() === handle.toLowerCase() && native[2] === post) || url.search || url.hash) invalid();
    authorId = handle.toLowerCase(); authorUrl = `https://x.com/${authorId}`;
  } else if (platform === "linkedin") {
    const handle = /^https:\/\/www\.linkedin\.com\/in\/([A-Za-z\d][A-Za-z\d_-]{0,99})\/$/u.exec(source)?.[1];
    if (handle && !["me", "self"].includes(handle.toLowerCase())) { authorId = handle; authorUrl = source; }
  } else if (platform === "facebook") {
    const profile = source.startsWith("https://www.facebook.com/profile.php?id=") ? source.slice("https://www.facebook.com/profile.php?id=".length) : "";
    if (id(profile)) { if (row.author_id !== undefined && row.author_id !== profile) invalid(); authorId = profile; authorUrl = source; }
  } else if (platform === "instagram" && field("provider") === "onecrew-instagram-native-v1") {
    if (!id(field("author_id"), 25) || !/^[a-z\d_.]{1,30}$/u.test(field("username")) || source !== `https://www.instagram.com/${field("username")}/`) invalid();
    authorId = field("author_id"); authorUrl = source;
  } else if (platform === "reddit" && field("provider") === "onecrew-reddit-native-v1") {
    if (!/^[a-z\d]{1,15}$/u.test(field("id")) || source !== `https://www.reddit.com/comments/${field("id")}/`) invalid();
    if (field("author_id") || field("author_url")) {
      if (!/^t2_[a-z\d]{1,15}$/u.test(field("author_id")) || !/^[A-Za-z\d_-]{3,20}$/u.test(field("author")) || field("author_url") !== `https://www.reddit.com/user/${field("author")}/`) invalid();
      authorId = field("author_id"); authorUrl = field("author_url");
    }
  } else if (platform === "douyin") {
    const post = url.pathname.startsWith("/video/") ? url.pathname.slice(7) : "";
    if (!id(post) || source !== `https://www.douyin.com/video/${post}` || url.search || url.hash) invalid();
  } else if (platform === "tieba") {
    if (!id(field("id")) || url.pathname !== `/p/${field("id")}` || url.hash) invalid();
  } else if (platform === "weibo") {
    const bound = (candidate: string) => /^https:\/\/weibo\.com\/([1-9]\d{0,19})\/([A-Za-z\d]{1,20})$/u.exec(candidate);
    const canonical = url.search === "?refer_flag=1001030103_" && !url.hash ? source.slice(0, source.indexOf("?")) : source;
    const match = bound(canonical);
    if (match && BigInt(match[1]!) <= BigInt(Number.MAX_SAFE_INTEGER)) { source = canonical; authorId = match[1]!; authorUrl = `https://weibo.com/u/${authorId}`; }
  }
  return { source_url: source, title, author_label: clean(author, 200), author_id: authorId, author_url: authorUrl };
}

export function decodeCreatorSearch(platform: string, value: unknown, limit: number): CreatorSearchHit[] {
  const source = creatorSearchSource(platform);
  if (!Number.isInteger(limit) || limit < 1 || limit > source.maximum) throw new OpenCliError("opencli_invalid_input");
  const rows = parse(z.array(z.unknown()).max(limit).max(50), value);
  const seen = new Set<string>();
  if (platform === "bilibili") {
    if (!rows.length) invalid();
    return rows.map(value => {
      const row = parse(bilibiliSearchVideoSchema, value);
      if (seen.has(row.bvid)) invalid(); seen.add(row.bvid);
      return { source_url: row.url, title: row.title, author_label: row.author, author_id: row.author_id, author_url: row.author_url };
    });
  }
  if (platform === "tiktok") return rows.map(value => {
    const row = parse(tiktokSearchVideoSchema, value);
    if (seen.has(row.id)) invalid(); seen.add(row.id);
    return { source_url: row.url, title: [...row.desc].slice(0, 500).map(character => /\p{Cc}/u.test(character) ? " " : character).join(""), author_label: row.author, author_id: row.author_id, author_url: `https://www.tiktok.com/@${row.author}` };
  });
  const output: CreatorSearchHit[] = [];
  for (const value of rows) {
    const hit = platform === "pinterest" ? pinterest(value) : generic(platform, value);
    if (!seen.has(hit.source_url)) { seen.add(hit.source_url); output.push(hit); }
  }
  return output;
}
