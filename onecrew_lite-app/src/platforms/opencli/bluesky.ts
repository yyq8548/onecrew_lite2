import { z } from "zod";
import { OpenCliError } from "./errors.ts";
import { publicWebUrl } from "./public-url.ts";
import type { ReviewedOpenCliRead } from "./read-plan.ts";

export function validBlueskyDid(value: string) { return value.length <= 256 && (/^did:plc:[a-z2-7]{24}$/u.test(value) || /^did:web:[A-Za-z\d._-]+(?::[A-Za-z\d._-]+)*$/u.test(value)); }
const handle = (value: string) => value.length <= 253 && value.includes(".") && value.split(".").every(part => part.length >= 1 && part.length <= 63 && /^[a-z\d](?:[a-z\d-]*[a-z\d])?$/u.test(part));
export function blueskyPostTarget(uri: string) {
  const match = /^at:\/\/(.+)\/app\.bsky\.feed\.post\/([A-Za-z\d_-]{1,64})$/u.exec(uri);
  return match && validBlueskyDid(match[1]!) ? { did: match[1]!, source: `https://bsky.app/profile/${match[1]}/post/${match[2]}` } : null;
}
export function blueskyUriFromSource(source: string) {
  const match = /^https:\/\/bsky\.app\/profile\/(.+)\/post\/([A-Za-z\d_-]{1,64})$/u.exec(source);
  if (!match) throw new OpenCliError("opencli_invalid_input");
  const uri = `at://${match[1]}/app.bsky.feed.post/${match[2]}`;
  if (blueskyPostTarget(uri)?.source !== source) throw new OpenCliError("opencli_invalid_input");
  return uri;
}
const text = z.string().refine(value => value.isWellFormed());
const bounded = (maximum: number, required = false) => text.refine(value => Buffer.byteLength(value) <= maximum && !value.includes("\0") && (!required || !!value.trim()));
function one<T extends z.ZodType>(schema: T, value: unknown): z.output<T> { const parsed = z.array(schema).length(1).safeParse(value); if (!parsed.success) throw new OpenCliError("opencli_invalid_output"); return parsed.data[0]!; }
export function blueskyProfilePlan(did: string) {
  if (!validBlueskyDid(did)) throw new OpenCliError("opencli_invalid_input");
  const schema = z.strictObject({ did: z.literal(did), handle: text.refine(handle), display_name: bounded(800, true), biography: bounded(8000), follower_count: z.number().int().nonnegative().safe().nullable(), profile_url: z.literal(`https://bsky.app/profile/${did}`) });
  return { site: "onecrew-bluesky", name: "profile", args: ["-f", "json", "--", did], addons: ["onecrew-bluesky"], public: true, maximumBytes: 32000, decode: (value: unknown) => one(schema, value) } satisfies ReviewedOpenCliRead<unknown>;
}
function publicLink(value: string) {
  if (!value.startsWith("mailto:")) return publicWebUrl(value) !== null;
  const email = value.slice(7); return Buffer.byteLength(email) <= 2041 && !/[?#%/\0 \n\r]/u.test(email) && email.includes("@");
}
export function blueskyPostPlan(uri: string, comments = false) {
  const target = blueskyPostTarget(uri); if (!target) throw new OpenCliError("opencli_invalid_input");
  const schema = z.strictObject({ author_id: z.literal(target.did), author_handle: text.refine(handle), post_uri: z.literal(uri), source_url: z.literal(target.source), text: bounded(16000, true), public_links: z.array(text.refine(publicLink)).max(10), comments: z.array(z.strictObject({ post_uri: text, source_url: text, text: bounded(2000, true) })).max(comments ? 5 : 0) }).refine(row => new Set(row.comments.map(comment => comment.post_uri)).size === row.comments.length && row.comments.every(comment => comment.post_uri !== uri && blueskyPostTarget(comment.post_uri)?.source === comment.source_url));
  return { site: "onecrew-bluesky", name: comments ? "comments" : "post", args: ["-f", "json", "--", uri], addons: ["onecrew-bluesky"], public: true, maximumBytes: 32000, decode: (value: unknown) => one(schema, value) } satisfies ReviewedOpenCliRead<unknown>;
}
