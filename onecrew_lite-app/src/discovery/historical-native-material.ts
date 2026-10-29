import { isIP } from "node:net";
import { z } from "zod";

const text = (maximum: number) => z.string().refine(value => value.isWellFormed() && Buffer.byteLength(value) <= maximum && !value.includes("\0"));
const label = text(512).refine(value => !!value.trim() && !/\p{Cc}/u.test(value));
const count = z.number().int().nonnegative().safe(), positive = z.number().int().positive().safe(), numeric = z.string().regex(/^[1-9][0-9]{0,19}$/u);
const hnAuthor = z.string().regex(/^[A-Za-z0-9_-]{1,64}$/u);
export function publicLink(value: string): boolean {
  if (Buffer.byteLength(value) > 2048 || /[\p{Cc}\p{White_Space}]/u.test(value)) return false;
  const url = URL.parse(value); if (!url) return false;
  const host = url.hostname; return url.protocol === "https:" && !url.username && !url.password && !url.port && isIP(host.replace(/^\[|\]$/gu, "")) === 0 && host.includes(".") && !host.endsWith(".") && !["localhost", "local", "internal", "test", "invalid", "onion", "home.arpa"].some(suffix => host === suffix || host.endsWith(`.${suffix}`));
}
const zhAuthor = { author_id: z.string().regex(/^[a-f0-9]{32}$/u), url_token: z.string().regex(/^[A-Za-z0-9_-]{1,128}$/u), author_label: label, author_url: z.string() };
const zhProfile = z.strictObject({ ...zhAuthor, follower_count: count.nullable(), bio: text(8000), bio_status: z.enum(["complete", "missing", "unknown"]).optional() }).refine(value => value.author_url === `https://www.zhihu.com/people/${value.url_token}`);
const pixivProfile = z.strictObject({ adapter_version: z.literal("1.0.0"), author_id: numeric, author_label: label, profile_url: z.string(), follower_count: z.null(), bio: text(8000), public_links: z.array(z.string().refine(publicLink)).max(10), evidence_source: z.literal("native_ajax_response") }).refine(value => value.profile_url === `https://www.pixiv.net/users/${value.author_id}`);
const hnProfile = z.strictObject({ author_id: hnAuthor, username: z.string(), biography_html: text(16000), biography: text(8000), profile_url: z.string(), created: count, karma: count, follower_count: z.null() }).refine(value => value.username === value.author_id && value.profile_url === `https://news.ycombinator.com/user?id=${value.author_id}`);

// 知乎复用已有严格证据合同；其他退役平台仅解码历史材料。
export function validateHistoricalDiscoveryProfile(platform: string, author: string, authorUrl: string, raw: unknown): Record<string, unknown> {
  const native = platform === "zhihu" ? zhProfile.parse(raw) : platform === "pixiv" ? pixivProfile.parse(raw) : platform === "hackernews" ? hnProfile.parse(raw) : null;
  if (!native || native.author_id !== author || ("author_url" in native ? native.author_url : native.profile_url) !== authorUrl) throw new Error("review_native_profile_binding_unavailable");
  return native;
}
export function validateHistoricalDiscoveryContent(platform: string, author: string, source: string, raw: unknown): Record<string, unknown> {
  if (platform === "pixiv") {
    const id = /^https:\/\/www\.pixiv\.net\/artworks\/([1-9][0-9]{0,19})$/u.exec(source)?.[1];
    if (!id || !numeric.safeParse(author).success) throw new Error("review_native_content_binding_unavailable");
    return z.strictObject({ post_id: z.literal(id), author_id: z.literal(author), source_url: z.literal(source), text: text(16000).refine(value => !!value.trim()), text_truncated: z.boolean() }).parse(raw);
  }
  if (platform === "hackernews") {
    const id = /^https:\/\/news\.ycombinator\.com\/item\?id=([1-9][0-9]*)$/u.exec(source)?.[1];
    if (!id || BigInt(id) > BigInt(Number.MAX_SAFE_INTEGER) || !hnAuthor.safeParse(author).success) throw new Error("review_native_content_binding_unavailable");
    return z.strictObject({ item_id: z.literal(Number(id)), author_id: z.literal(author), source_url: z.literal(source), title: text(2000).refine(value => !!value.trim()), text_html: text(16000), text: text(16000).refine(value => !!value.trim()), external_url: z.string().refine(value => Buffer.byteLength(value) <= 2048 && !/\p{Cc}/u.test(value)).nullable(), comment_ids: z.array(positive).max(5).refine(values => new Set(values).size === values.length && !values.includes(Number(id))), comments: z.array(z.unknown()).length(0) }).parse(raw);
  }
  if (platform !== "zhihu") throw new Error("review_platform_unsupported");
  const link = z.string().refine(value => {
    if (Buffer.byteLength(value) > 2048 || /\p{Cc}/u.test(value)) return false;
    const url = URL.parse(value); if (!url || url.username || url.password || url.port || [...url.searchParams.keys()].some(key => ["token", "access_token", "api_key", "authorization", "cookie", "session", "password", "secret", "auth"].includes(key.toLowerCase()))) return false;
    return url.protocol === "mailto:" ? !url.search && !url.hash && url.pathname.includes("@") && !/[/%]/u.test(url.pathname) : url.protocol === "https:" && isIP(url.hostname.replace(/^\[|\]$/gu, "")) === 0 && url.hostname.includes(".") && ![".local", ".internal", ".localhost"].some(suffix => url.hostname.endsWith(suffix));
  });
  const common = { ...zhAuthor, source_url: z.literal(source), text: text(16000).refine(value => !!value.trim()), public_links: z.array(link).max(10).refine(values => new Set(values).size === values.length) };
  const native = z.union([z.strictObject({ ...common, article_id: numeric, title: text(2000) }), z.strictObject({ ...common, answer_id: numeric, question_id: numeric })]).parse(raw);
  const expectedSource = "article_id" in native ? `https://zhuanlan.zhihu.com/p/${native.article_id}` : `https://www.zhihu.com/question/${native.question_id}/answer/${native.answer_id}`;
  if (native.author_id !== author || native.author_url !== `https://www.zhihu.com/people/${native.url_token}` || expectedSource !== source) throw new Error("review_native_content_binding_unavailable"); return native;
}
