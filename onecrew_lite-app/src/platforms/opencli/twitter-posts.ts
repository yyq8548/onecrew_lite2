import { z } from "zod";
import { nativeContentObservations } from "./observations.ts";
import { OpenCliError } from "./errors.ts";
import type { ReviewedOpenCliRead } from "./read-plan.ts";

const text = z.string().refine(value => value.isWellFormed());
const numeric = (value: string) => /^[1-9]\d{0,19}$/u.test(value);
const handleValid = (value: string) => /^[A-Za-z\d_]{1,15}$/u.test(value);
const invalid = (): never => { throw new OpenCliError("opencli_invalid_output"); };
function target(handle: string, postId: string, accountId?: string) {
  if (!handleValid(handle) || handle !== handle.toLowerCase() || !numeric(postId) || (accountId !== undefined && !numeric(accountId))) throw new OpenCliError("opencli_invalid_input");
}
const identity = z.strictObject({ adapter_version: z.literal("1.0.0"), post_id: text, account_id: text, handle: text, profile_url: text, source_url: text, identity_kind: z.literal("native_post_author_id"), evidence_source: z.literal("x_tweet_detail") });
function validIdentity(row: z.infer<typeof identity>, handle: string, postId: string) {
  return numeric(row.account_id) && row.post_id === postId && row.handle === handle && row.profile_url === `https://x.com/${handle}` && row.source_url === `https://x.com/${handle}/status/${postId}`;
}
function one<T extends z.ZodType>(schema: T, value: unknown): z.output<T> {
  const parsed = z.array(schema).length(1).safeParse(value);
  if (!parsed.success) return invalid();
  return parsed.data[0]!;
}
export function twitterPostIdentityPlan(handle: string, postId: string) {
  target(handle, postId);
  return { site: "onecrew-twitter-post-identity", name: "read", args: ["-f", "json", "--", handle, postId], addons: ["onecrew-twitter-post-identity"], maximumBytes: 4000, decode(value: unknown) {
    const row = one(identity, value);
    if (!validIdentity(row, handle, postId)) return invalid();
    return row;
  } } satisfies ReviewedOpenCliRead<unknown>;
}
const content = z.strictObject({ adapter_version: z.literal("1.0.0"), identity, text, text_truncated: z.boolean(), text_source: z.enum(["note_tweet", "legacy_full_text"]), created_at: text, has_media: z.boolean(), quotes_another_post: z.boolean(), expanded_urls: z.array(text).max(20), observations: nativeContentObservations.optional() });
function locator(value: string) {
  if (!value || Buffer.byteLength(value) > 2048 || /[\p{Cc}\s]/u.test(value)) return false;
  const parsed = URL.parse(value);
  return parsed !== null && ["http:", "https:"].includes(parsed.protocol) && Boolean(parsed.hostname) && !parsed.username && !parsed.password;
}
export function twitterPostContentPlan(handle: string, postId: string, accountId: string) {
  target(handle, postId, accountId);
  return { site: "onecrew-twitter-post-content", name: "read", args: ["-f", "json", "--", handle, postId, accountId], addons: ["onecrew-twitter-post-content", "onecrew-twitter-post-identity", "onecrew-browser-search"], maximumBytes: 64000, decode(value: unknown) {
    const row = one(content, value);
    if (!validIdentity(row.identity, handle, postId) || row.identity.account_id !== accountId || !row.text.trim() || Buffer.byteLength(row.text) > 8000 || row.text.includes("\0") || !row.created_at.trim() || Buffer.byteLength(row.created_at) > 80 || /\p{Cc}/u.test(row.created_at) || new Set(row.expanded_urls).size !== row.expanded_urls.length || !row.expanded_urls.every(locator)) return invalid();
    return row;
  } } satisfies ReviewedOpenCliRead<unknown>;
}
export function twitterLanguagePlan(handle: string, postId: string, accountId: string) {
  target(handle, postId, accountId);
  return { site: "onecrew-twitter-post-content", name: "read", args: ["-f", "json", "--language-sample", "--", handle, postId, accountId], addons: ["onecrew-twitter-post-content", "onecrew-twitter-post-identity", "onecrew-browser-search"], maximumBytes: 64000, decode(value: unknown) {
    const row = one(z.record(z.string(), z.unknown()), value);
    if (row.author_id !== accountId || row.source_url !== `https://x.com/${handle}/status/${postId}`) return invalid();
    return row;
  } } satisfies ReviewedOpenCliRead<unknown>;
}
const recent = z.object({ id: text, author: text, url: text, is_retweet: z.boolean(), has_media: z.boolean(), created_at: text, text, quoted_tweet: z.unknown().optional() });
export function twitterRecentContentPlan(handle: string) {
  if (!handleValid(handle) || handle !== handle.toLowerCase()) throw new OpenCliError("opencli_invalid_input");
  return { site: "twitter", name: "tweets", args: ["-f", "json", "--limit", "3", "--", handle], addons: [], maximumBytes: 160000, decode(value: unknown) {
    const parsed = z.array(recent).min(1).max(3).safeParse(value);
    if (!parsed.success) return invalid();
    const seen = new Set<string>();
    let excluded_retweets = 0;
    const original_posts: { id: string; source_url: string; text: string; text_truncated: boolean; created_at: string; has_media: boolean; quotes_another_post: boolean }[] = [];
    for (const row of parsed.data) {
      if (!handleValid(row.author) || !/^\d{1,20}$/u.test(row.id) || seen.has(row.id) || row.url.toLowerCase() !== `https://x.com/${row.author}/status/${row.id}`.toLowerCase() || Buffer.byteLength(row.created_at) > 80 || /\p{Cc}/u.test(row.created_at) || Buffer.byteLength(row.text) > 40000 || row.text.includes("\0")) return invalid();
      seen.add(row.id);
      if (row.author.toLowerCase() !== handle) continue;
      if (row.is_retweet) { excluded_retweets++; continue; }
      let excerpt = ""; let bytes = 0;
      for (const character of row.text) { const length = Buffer.byteLength(character); if (bytes + length > 2000) break; bytes += length; excerpt += character; }
      original_posts.push({ id: row.id, source_url: `https://x.com/${handle}/status/${row.id}`, text: excerpt, text_truncated: bytes < Buffer.byteLength(row.text), created_at: row.created_at, has_media: row.has_media, quotes_another_post: row.quoted_tweet !== undefined && row.quoted_tweet !== null });
    }
    return { author_id: handle, profile_url: `https://x.com/${handle}`, sampled_posts: original_posts.length + excluded_retweets, excluded_retweets, original_posts, identity_kind: "mutable_handle" as const };
  } } satisfies ReviewedOpenCliRead<unknown>;
}
