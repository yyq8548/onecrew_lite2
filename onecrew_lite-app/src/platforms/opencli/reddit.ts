import { z } from "zod";
import { nativeContentObservations } from "./observations.ts";
import { OpenCliError } from "./errors.ts";
import { nativeCommentsPlan } from "./native-comments.ts";
import type { ReviewedOpenCliRead } from "./read-plan.ts";

const text = z.string().refine(value => value.isWellFormed());
const invalid = (): never => { throw new OpenCliError("opencli_invalid_output"); };
function target(username: string, authorId: string) { if (!/^[A-Za-z\d_-]{3,20}$/u.test(username) || !/^t2_[a-z\d]{1,15}$/u.test(authorId)) throw new OpenCliError("opencli_invalid_input"); }
function postId(source: string) { const match = /^https:\/\/www\.reddit\.com\/comments\/([a-z\d]{1,15})\/$/u.exec(source); if (!match) throw new OpenCliError("opencli_invalid_input"); return match[1]!; }
function excerpt(value: unknown, maximum: number) {
  const parsed = text.safeParse(value); if (!parsed.success) return invalid();
  if (parsed.data.includes("\0")) return invalid();
  let output = ""; let bytes = 0;
  for (const character of parsed.data) { const length = Buffer.byteLength(character); if (bytes + length > maximum) break; bytes += length; output += character; }
  return output;
}
export function redditProfilePlan(username: string, authorId: string) {
  target(username, authorId);
  const schema = z.strictObject({ adapter_version: z.literal("1.0.0"), id: z.literal(authorId), username: text.refine(value => value.toLowerCase() === username.toLowerCase()), biography: text.refine(value => [...value].length <= 2000 && !value.includes("\0")), follower_count: z.number().int().nonnegative().safe().nullable().optional().transform(value => value ?? null), profile_url: text, evidence_source: z.literal("native_profile_response") }).refine(row => row.profile_url === `https://www.reddit.com/user/${row.username}/`);
  return { site: "onecrew-reddit-profile", name: "read", args: ["-f", "json", "--", username, authorId], addons: ["onecrew-browser-search"], maximumBytes: 16000, decode(value: unknown) { const parsed = z.array(schema).length(1).safeParse(value); return parsed.success ? parsed.data[0]! : invalid(); } } satisfies ReviewedOpenCliRead<unknown>;
}
export function redditRecentPlan(username: string, authorId: string) {
  target(username, authorId);
  return { site: "onecrew-reddit-profile", name: "recent", args: ["-f", "json", "--", username, authorId], addons: ["onecrew-browser-search"], maximumBytes: 16000, decode(value: unknown) {
    const parsed = z.array(z.strictObject({ source_url: text.regex(/^https:\/\/www\.reddit\.com\/comments\/[a-z\d]{1,15}\/$/u) })).max(3).safeParse(value);
    if (!parsed.success) return invalid();
    const urls = parsed.data.map(row => row.source_url); return new Set(urls).size === urls.length ? urls : invalid();
  } } satisfies ReviewedOpenCliRead<unknown>;
}
function project(rows: Record<string, unknown>[], username: string, authorId: string, source: string) {
  const id = postId(source); const post = rows[0];
  if (!post || rows.length > 21 || post.type !== "POST" || post.id !== `t3_${id}` || post.native_author_id !== authorId || post.source_url !== source || !/^t2_[a-z\d]{1,15}$/u.test(authorId) || typeof post.author !== "string" || post.author.toLowerCase() !== username.toLowerCase()) return invalid();
  const original = excerpt(post.text, 8000); const seen = new Set<string>();
  const comments = rows.slice(1).map(row => {
    const url = row.source_url; if (typeof url !== "string" || !url.startsWith(`${source}_/`)) return invalid();
    const comment = /^([a-z\d]{1,15})\/$/u.exec(url.slice(source.length + 2)); const body = excerpt(row.text, 2000);
    if (!comment || row.type !== "L0" || row.id !== `t1_${comment[1]}` || !body.trim() || seen.has(url)) return invalid();
    seen.add(url); return { source_url: url, text: body };
  });
  if (!original.trim() && !comments.length) return invalid();
  return { author_id: authorId, source_url: source, text: original, comments };
}
export function redditLanguagePlan(username: string, authorId: string, source: string) {
  const plan = nativeCommentsPlan({ kind: "reddit_post", source: postId(source) }, 5);
  return { ...plan, decode(value: unknown) { return project(plan.decode(value), username, authorId, source); } };
}
export function redditPostPlan(authorId: string, source: string) {
  const id = postId(source); const plan = nativeCommentsPlan({ kind: "reddit_post", source: id }, 1);
  return { ...plan, decode(value: unknown) {
    const rows = plan.decode(value); const post = rows[0];
    if (!post || typeof post.author !== "string" || typeof post.text_truncated !== "boolean") return invalid();
    target(post.author, authorId);
    const sample = project([post], post.author, authorId, source);
    return { post_id: id, author_id: authorId, source_url: source, text: sample.text, text_truncated: post.text_truncated || Buffer.byteLength(post.text as string) > 8000, ...(post.observations ? { observations: nativeContentObservations.parse(post.observations) } : {}) };
  } };
}
