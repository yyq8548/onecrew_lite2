import { z } from "zod";
import { OpenCliError } from "./errors.ts";
import { publicWebUrl } from "./public-url.ts";
import type { ReviewedOpenCliRead } from "./read-plan.ts";

const text = z.string().refine(value => value.isWellFormed());
const bounded = (maximum: number) => text.refine(value => Buffer.byteLength(value) <= maximum && !value.includes("\0"));
const authorValid = (value: string) => /^MS4wLjABAAAA[A-Za-z\d_-]{8,88}$/u.test(value);
const videoValid = (value: string) => /^[1-9]\d{0,19}$/u.test(value);
const label = text.refine(value => !!value.trim() && Buffer.byteLength(value) <= 512 && !/\p{Cc}/u.test(value));
function input(valid: boolean) { if (!valid) throw new OpenCliError("opencli_invalid_input"); }
function one<T extends z.ZodType>(schema: T, value: unknown): z.output<T> { const parsed = z.array(schema).length(1).safeParse(value); if (!parsed.success) throw new OpenCliError("opencli_invalid_output"); return parsed.data[0]!; }
function followers(value: string) {
  if (!/^(?:0|[1-9]\d*|[1-9]\d{0,2}(?:,\d{3})+)$/u.test(value)) return null;
  const count = Number(value.replace(/,/gu, "")); return Number.isSafeInteger(count) ? count : null;
}
export function douyinProfilePlan(author: string) {
  input(authorValid(author));
  const schema = z.strictObject({ adapter_version: z.literal("1.0.0"), author_id: z.literal(author), author_label: label, profile_url: z.literal(`https://www.douyin.com/user/${author}`), follower_count: z.number().int().nonnegative().safe().nullable().optional().transform(value => value ?? null), follower_text: bounded(100), bio: bounded(8000), bio_truncated: z.boolean(), public_links: z.array(text.refine(value => publicWebUrl(value) !== null)).max(10), evidence_source: z.literal("visible_profile_header") }).refine(row => row.follower_count === followers(row.follower_text));
  return { site: "onecrew-douyin-reader", name: "profile", args: ["-f", "json", "--", author], addons: ["onecrew-browser-search"], maximumBytes: 16000, decode: (value: unknown) => one(schema, value) } satisfies ReviewedOpenCliRead<unknown>;
}
export function douyinContentPlan(video: string, author?: string) {
  input(videoValid(video) && (author === undefined || authorValid(author)));
  const schema = z.strictObject({ post_id: z.literal(video), author_id: text.refine(value => authorValid(value) && (author === undefined || value === author)), author_label: label, profile_url: text, source_url: z.literal(`https://www.douyin.com/video/${video}`), text: bounded(12000).refine(value => !!value.trim()), text_truncated: z.boolean(), evidence_source: z.literal("visible_native_video_caption") }).refine(row => row.profile_url === `https://www.douyin.com/user/${row.author_id}`);
  return { site: "onecrew-douyin-reader", name: "content", args: ["-f", "json", "--", video, ...(author === undefined ? [] : [author])], addons: ["onecrew-browser-search"], maximumBytes: 16000, decode: (value: unknown) => one(schema, value) } satisfies ReviewedOpenCliRead<unknown>;
}
export function douyinCommentsPlan(video: string, author: string) {
  input(videoValid(video) && authorValid(author));
  const schema = z.strictObject({ post_id: z.literal(video), author_id: z.literal(author), source_url: z.literal(`https://www.douyin.com/video/${video}`), text: bounded(12000).refine(value => !!value.trim()), text_truncated: z.boolean(), comments: z.array(z.strictObject({ id: text.refine(videoValid), parent_post_id: z.literal(video), text: bounded(2000) })).max(5), evidence_source: z.literal("visible_same_video_comments") }).refine(row => new Set(row.comments.map(comment => comment.id)).size === row.comments.length);
  return { site: "onecrew-douyin-reader", name: "comments", args: ["-f", "json", "--", video, author], addons: ["onecrew-browser-search"], maximumBytes: 16000, decode: (value: unknown) => one(schema, value) } satisfies ReviewedOpenCliRead<unknown>;
}
