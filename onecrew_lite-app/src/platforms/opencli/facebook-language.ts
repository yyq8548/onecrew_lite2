import { z } from "zod";
import { OpenCliError } from "./errors.ts";
import { facebookIdValid, facebookSourceValid } from "./facebook-profile.ts";
import { nativeCommentsPlan } from "./native-comments.ts";
import type { ReviewedOpenCliRead } from "./read-plan.ts";

const text = z.string().refine(value => value.isWellFormed());
const invalid = (): never => { throw new OpenCliError("opencli_invalid_output"); };
function target(id: string, source?: string) { if (!facebookIdValid(id) || source !== undefined && !facebookSourceValid(source)) throw new OpenCliError("opencli_invalid_input"); }
function reel(source: string) { const match = /^https:\/\/www\.facebook\.com\/reel\/([1-9]\d{0,24})$/u.exec(source); if (!match) throw new OpenCliError("opencli_invalid_input"); return match[1]!; }
function excerpt(value: string, characters: number, maximum: number) {
  if (!value.isWellFormed() || !value.trim() || value.includes("\0") || [...value].length > characters) return invalid();
  let output = ""; let bytes = 0;
  for (const character of value) { const length = Buffer.byteLength(character); if (bytes + length > maximum) break; bytes += length; output += character; }
  return { text: output, truncated: bytes < Buffer.byteLength(value) };
}
function one<T extends z.ZodType>(schema: T, value: unknown): z.output<T> { const parsed = z.array(schema).length(1).safeParse(value); return parsed.success ? parsed.data[0]! : invalid(); }
export function facebookRecentPlan(authorId: string, base?: string) {
  target(authorId, base);
  return { site: base === undefined ? "onecrew-comments-facebook" : "onecrew-facebook-profile", name: base === undefined ? "recent" : "native-targets", args: ["-f", "json", "--", authorId, ...(base === undefined ? [] : [base])], addons: ["onecrew-browser-search"], maximumBytes: 2048, decode(value: unknown) {
    const parsed = z.array(z.strictObject({ author_id: z.literal(authorId), source_url: text.regex(/^https:\/\/www\.facebook\.com\/reel\/[1-9]\d{0,24}$/u) })).min(1).max(3).safeParse(value);
    if (!parsed.success) return invalid();
    const urls = parsed.data.map(row => row.source_url); return new Set(urls).size === urls.length ? urls : invalid();
  } } satisfies ReviewedOpenCliRead<unknown>;
}
const post = z.strictObject({ type: z.literal("POST"), source_url: text, native_author_id: text, author_url: text, identity_source: text, text, text_truncated: z.boolean() });
const comment = z.strictObject({ type: z.literal("COMMENT"), source_url: text, text, text_truncated: z.boolean() });
function project(value: unknown, source: string, authorId: string) {
  const id = reel(source); target(authorId);
  const parsed = z.array(z.union([post, comment])).min(1).max(6).safeParse(value); if (!parsed.success) return invalid();
  const first = parsed.data[0]!;
  if (first.type !== "POST" || first.source_url !== source || first.native_author_id !== authorId || first.author_url !== `https://www.facebook.com/profile.php?id=${authorId}` || first.identity_source !== "visible_numeric_profile_link") return invalid();
  const original = excerpt(first.text, 8000, 8000); const seen = new Set<string>();
  const comments = parsed.data.slice(1).map(row => {
    if (row.type !== "COMMENT" || !row.source_url.startsWith(`${source}?comment_id=`)) return invalid();
    const id = row.source_url.slice(source.length + 12);
    if (!/^[1-9]\d{0,24}$/u.test(id) || seen.has(id)) return invalid();
    seen.add(id); return { source_url: row.source_url, text: excerpt(row.text, 2000, 2000).text };
  });
  return { content: { post_id: id, author_id: authorId, source_url: source, text: original.text, text_truncated: first.text_truncated || original.truncated }, sample: { author_id: authorId, source_url: source, text: original.text, comments } };
}
export function facebookReelPlan(authorId: string, source: string) {
  reel(source); target(authorId);
  return { site: "onecrew-comments-facebook", name: "content", args: ["-f", "json", "--", source], addons: ["onecrew-browser-search"], maximumBytes: 40000, decode(value: unknown) {
    const parsed = z.array(post).length(1).safeParse(value); return parsed.success ? project(parsed.data, source, authorId).content : invalid();
  } } satisfies ReviewedOpenCliRead<unknown>;
}
export function facebookLanguagePlan(authorId: string, source: string) {
  reel(source); target(authorId); const plan = nativeCommentsPlan({ kind: "facebook_reel", source }, 5);
  return { ...plan, decode(value: unknown) { return project(plan.decode(value), source, authorId).sample; } };
}
export function facebookNativeOriginalPlan(authorId: string, base: string, source: string) {
  target(authorId, base); const id = reel(source);
  const schema = z.strictObject({ post_id: z.literal(id), author_id: z.literal(authorId), source_url: z.literal(source), text, text_truncated: z.boolean() });
  return { site: "onecrew-facebook-profile", name: "native-original", args: ["-f", "json", "--", authorId, base, source], addons: ["onecrew-browser-search"], maximumBytes: 80000, decode(value: unknown) {
    const row = one(schema, value); const original = excerpt(row.text, 8000, 8000); return { ...row, text: original.text, text_truncated: row.text_truncated || original.truncated };
  } } satisfies ReviewedOpenCliRead<unknown>;
}
