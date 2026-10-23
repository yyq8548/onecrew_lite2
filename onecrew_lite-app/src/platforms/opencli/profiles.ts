import { z } from "zod";
import { OpenCliError } from "./errors.ts";
import type { ReviewedOpenCliRead } from "./read-plan.ts";
import { instagramVisiblePosts, instagramVisiblePostsMatchUsername } from "./instagram-visible-contract.ts";
import { publicWebUrl } from "./public-url.ts";

const text = z.string().refine(value => value.isWellFormed());
const count = z.number().int().nonnegative().safe();
const byteText = (maximum: number) => text.refine(value => Buffer.byteLength(value) <= maximum && !/[\p{Cc}&&[^\n\r\t]]/v.test(value));
const charText = (maximum: number, multiline = false) => text.refine(value => [...value].length <= maximum && !(multiline ? /[\p{Cc}&&[^\n]]/v : /\p{Cc}/u).test(value));
const numeric = (value: string, maximum: number) => value.length <= maximum && /^[1-9]\d*$/u.test(value);
const invalidInput = (): never => { throw new OpenCliError("opencli_invalid_input"); };
function one<T>(schema: z.ZodType<T>, value: unknown): T { const result = z.array(schema).length(1).safeParse(value); if (!result.success) throw new OpenCliError("opencli_invalid_output"); return result.data[0]!; }

export function tiktokProfilePlan(username: string, authorId: string) {
  if (!/^[A-Za-z\d_.]{1,24}$/u.test(username) || !numeric(authorId, 25)) invalidInput();
  const link = byteText(2048).refine(value => publicWebUrl(value, true) !== null);
  const schema = z.object({ adapter_version: z.literal("1.0.0"), evidence_source: z.literal("profile_bootstrap"), id: z.literal(authorId), username: z.literal(username), display_name: byteText(1000).refine(value => !!value.trim()), signature: byteText(16000), follower_count: count, verified: z.boolean(), private_account: z.boolean(), profile_url: z.literal(`https://www.tiktok.com/@${username}`), contact_links: z.array(link).max(10).optional() }).strict();
  return { site: "onecrew-tiktok-profile", name: "read", args: ["--author-id", authorId, "-f", "json", "--", username], addons: ["onecrew-tiktok-profile"], decode: (value: unknown) => one(schema, value) } satisfies ReviewedOpenCliRead<unknown>;
}
export function bilibiliProfilePlan(mid: string) {
  if (!numeric(mid, 20)) invalidInput();
  const schema = z.object({ adapter_version: z.literal("1.0.0"), mid: z.literal(mid), name: charText(200).refine(value => !!value.trim()), follower_count: count, follower_count_text: text, follower_display: charText(32).refine(value => !!value.trim()), profile_url: z.literal(`https://space.bilibili.com/${mid}`), evidence_source: z.literal("profile_tooltip"), sign: byteText(8000).nullable().optional(), bio_status: z.enum(["complete", "missing", "unavailable", "truncated"]).optional(), bio_truncated: z.boolean().optional(), signature_status: z.enum(["complete", "missing", "unavailable"]).optional(), announcement_status: z.enum(["complete", "missing", "unavailable"]).optional() }).strict().refine(row => row.follower_count_text.length <= 32 && /^(?:0|[1-9]\d*|[1-9]\d{0,2}(?:,\d{3})+)$/u.test(row.follower_count_text) && Number(row.follower_count_text.replace(/,/gu, "")) === row.follower_count);
  return { site: "onecrew-bilibili-profile", name: "read", args: ["-f", "json", "--", mid], addons: ["onecrew-bilibili-profile"], decode(value: unknown) { const { sign, ...result } = one(schema, value); return { ...result, ...(sign == null ? {} : { sign }) }; } } satisfies ReviewedOpenCliRead<unknown>;
}
export function instagramProfilePlan(inputUsername: string, authorId: string) {
  if (!/^[A-Za-z\d_.]{1,30}$/u.test(inputUsername) || !numeric(authorId, 25) && authorId !== `https://www.instagram.com/${inputUsername.toLowerCase()}/`) invalidInput();
  const username = inputUsername.toLowerCase();
  const link = text.refine(value => {
    if (Buffer.byteLength(value) > 2048 || /\p{Cc}/u.test(value) || !z.url().safeParse(value).success) return false;
    const url = new URL(value);
    return ["http:", "https:"].includes(url.protocol) && !!url.hostname && !url.username && !url.password;
  });
  const schema = z.object({ adapter_version: z.literal("1.0.0"), id: z.literal(authorId), username: z.literal(username), display_name: text.refine(value => [...value].length <= 200 && !value.includes("\0")), biography: text.refine(value => [...value].length <= 2000 && !value.includes("\0")), public_email: text.refine(value => Buffer.byteLength(value) <= 320 && !/\p{Cc}/u.test(value) && value.includes("@")).nullable().optional(), contact_links: z.array(link).max(10).default([]), follower_count: count.nullable().default(null), follower_text: text.max(80).optional(), private_account: z.boolean().nullable().default(null), verified: z.boolean().nullable().default(null), web_text: text.refine(value => [...value].length <= 4000 && !value.includes("\0")), profile_url: z.literal(`https://www.instagram.com/${username}/`), evidence_source: z.literal("public_profile_dom_search_bound") }).strict();
  return { site: "onecrew-instagram-profile", name: "read", args: ["-f", "json", "--", username, authorId], addons: ["onecrew-browser-search"], maximumBytes: 16000, decode(value: unknown) {
    const pageIdentity = authorId === `https://www.instagram.com/${username}/`;
    const { public_email, contact_links, ...result } = one(schema.extend({ evidence_source: z.literal(pageIdentity ? "public_profile_dom_profile_bound" : "public_profile_dom_search_bound"), identity_kind: pageIdentity ? z.literal("profile_url") : z.literal("native_id").optional(), visible_posts: instagramVisiblePosts.optional(), execution_mode: z.enum(["opencli_dom", "computer_use"]).optional(), bio_truncated: z.boolean().optional(), bio_status: z.enum(["complete", "truncated", "missing"]).optional() }).refine(row => instagramVisiblePostsMatchUsername(row.visible_posts, username)), value);
    return { ...result, ...(public_email == null ? {} : { public_email }), ...(contact_links.length ? { contact_links } : {}) };
  } } satisfies ReviewedOpenCliRead<unknown>;
}
export function twitterProfilePlan(handle: string) {
  if (!/^[a-z\d_]{1,15}$/u.test(handle)) invalidInput();
  return { site: "twitter", name: "profile", args: ["-f", "json", "--", handle], addons: [], decode(value: unknown) {
    const row = one(z.record(z.string(), z.unknown()), value);
    const field = (name: string) => typeof row[name] === "string" ? row[name] : "";
    const schema = z.object({ screen_name: text.regex(/^[A-Za-z\d_]{1,15}$/u).refine(value => value.toLowerCase() === handle), name: charText(200).refine(value => !!value.trim()), bio: charText(2000, true), location: charText(200), followers: count });
    const parsed = schema.safeParse({ screen_name: field("screen_name"), name: field("name"), bio: field("bio"), location: field("location"), followers: row.followers });
    if (!parsed.success) throw new OpenCliError("opencli_invalid_output");
    return { handle, profile_url: `https://x.com/${handle}`, name: parsed.data.name, bio: parsed.data.bio, self_declared_location: parsed.data.location, follower_count: parsed.data.followers > 0 ? parsed.data.followers : null, identity_kind: "mutable_handle" as const };
  } } satisfies ReviewedOpenCliRead<unknown>;
}
export function twitterIdentityPlan(handle: string, evidenceSource: "x_user_by_screen_name" | "x_tweet_detail" = "x_user_by_screen_name") {
  if (!/^[a-z\d_]{1,15}$/u.test(handle)) invalidInput();
  const schema = z.object({ adapter_version: z.literal("1.0.0"), account_id: text.refine(value => numeric(value, 20)), handle: z.literal(handle), profile_url: z.literal(`https://x.com/${handle}`), name: charText(200).refine(value => !!value.trim()), bio: charText(2000, true), self_declared_location: charText(200), follower_count: count.nullable().default(null), identity_kind: z.literal("native_id"), evidence_source: z.literal(evidenceSource) }).strict();
  return { site: "onecrew-twitter-identity", name: "read", args: ["-f", "json", "--", handle], addons: ["onecrew-twitter-identity"], maximumBytes: 16000, decode: (value: unknown) => one(schema, value) } satisfies ReviewedOpenCliRead<unknown>;
}
