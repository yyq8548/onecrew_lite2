import { z } from "zod";
import { OpenCliError } from "./errors.ts";
import type { ReviewedOpenCliRead } from "./read-plan.ts";

const text = z.string().refine(value => value.isWellFormed());
const outputError = (): never => { throw new OpenCliError("opencli_invalid_output"); };
const inputError = (): never => { throw new OpenCliError("opencli_invalid_input"); };
function parsed<T>(schema: z.ZodType<T>, value: unknown): T { const result = schema.safeParse(value); return result.success ? result.data : outputError(); }
const channelValid = (value: string) => /^UC[A-Za-z\d_-]{8,62}$/u.test(value) || /^@[A-Za-z\d_.-]{1,100}$/u.test(value);
const idValid = (value: string) => /^[A-Za-z\d_-]{11}$/u.test(value);
function youtubeUrl(value: string): URL | null {
  if (!z.url().safeParse(value).success) return null;
  const url = new URL(value);
  return url.protocol === "https:" && ["youtube.com", "www.youtube.com"].includes(url.hostname) && !url.username && !url.password && !url.port ? url : null;
}
export function youtubeVideoId(value: string): string {
  if (idValid(value)) return value;
  const url = youtubeUrl(value);
  if (!url) return inputError();
  const ids = url.searchParams.getAll("v");
  if (url.pathname === "/watch" && ids.length === 1 && idValid(ids[0]!)) return ids[0]!;
  const path = /^\/(?:shorts|live)\/([A-Za-z\d_-]{11})$/u.exec(url.pathname);
  return path?.[1] ?? inputError();
}
function fields(value: unknown): Map<string, string> {
  const rows = parsed(z.array(z.object({ field: text, value: text })).max(100), value);
  const output = new Map<string, string>();
  for (const row of rows) { if (output.has(row.field)) outputError(); output.set(row.field, row.value); }
  return output;
}
const required = (value: Map<string, string>, key: string) => value.get(key)?.trim() ? value.get(key)! : outputError();

export function youtubeChannelPlan(channel: string) {
  if (!channelValid(channel)) inputError();
  return { site: "youtube", name: "channel", args: ["--limit", "10", "-f", "json", "--", channel], addons: [], decode(value: unknown) {
    const rows = parsed(z.array(z.object({ field: text, value: text })).max(100), value);
    const recent = rows.findIndex(row => row.field === "---" && row.value === "--- Recent Videos ---");
    const data = fields(recent < 0 ? rows : rows.slice(0, recent));
    const channelId = required(data, "channelId");
    const url = required(data, "url");
    if (!channelValid(channelId) || !youtubeUrl(url)) outputError();
    return { channel_id: channelId, name: required(data, "name"), handle: data.get("handle") ?? "", description: data.get("description") ?? "", subscribers_raw: data.get("subscribers") ?? "", url, recent_videos: recent < 0 ? [] : rows.slice(recent + 1).map(row => ({ title: row.field, summary: row.value })) };
  } } satisfies ReviewedOpenCliRead<unknown>;
}
export function youtubeVideoPlan(video: string) {
  const id = youtubeVideoId(video);
  return { site: "youtube", name: "video", args: ["-f", "json", "--", id], addons: [], decode(value: unknown) {
    const data = fields(value);
    const channel = required(data, "channelId");
    if (required(data, "videoId") !== id || !channel.startsWith("UC") || !channelValid(channel)) outputError();
    return { video_id: id, channel_id: channel, title: required(data, "title"), channel: required(data, "channel"), description: data.get("description") ?? "", subscribers_raw: data.get("subscribers") ?? "", url: `https://www.youtube.com/watch?v=${id}` };
  } } satisfies ReviewedOpenCliRead<unknown>;
}
export function youtubeSearchPlan(query: string, limit: number) {
  if (!query.isWellFormed() || !query.trim() || [...query].length > 160 || /\p{Cc}/u.test(query) || !Number.isInteger(limit) || limit < 1 || limit > 50) inputError();
  return { site: "onecrew-search-youtube", name: "search", args: ["--limit", String(limit), "-f", "json", "--", query], addons: ["onecrew-browser-search"], emptyOn66: () => [] as z.infer<typeof schema>[], decode(value: unknown) { return parsed(z.array(schema).max(50), value); } } satisfies ReviewedOpenCliRead<unknown>;
}
const schema = z.object({ rank: z.number().int().nonnegative().max(4294967295).nullable().default(null), title: text.default(""), channel: text.default(""), views: text.default(""), duration: text.default(""), published: text.default(""), url: text }).refine(row => youtubeUrl(row.url) !== null && !!row.title.trim() && [...row.title].length <= 500 && !/\p{Cc}/u.test(row.title) && [...row.channel].length <= 200 && !/\p{Cc}/u.test(row.channel));
export function youtubeTranscriptPlan(video: string) {
  const id = youtubeVideoId(video);
  const line = z.object({ timestamp: text.regex(/^[\d:]{1,24}$/u), speaker: text.default("").refine(value => [...value].length <= 120), text: text.refine(value => !!value.trim() && [...value].length <= 4000) });
  return { site: "youtube", name: "transcript", args: ["-f", "json", "--", id], addons: [], emptyOn66: () => [] as z.infer<typeof line>[], decode(value: unknown) { return parsed(z.array(line).min(1).max(2000).refine(lines => lines.reduce((sum, line) => sum + Buffer.byteLength(line.text), 0) <= 250000), value); } } satisfies ReviewedOpenCliRead<unknown>;
}
export function youtubeLanguagePlan(channel: string, video: string) {
  if (!/^UC[A-Za-z\d_-]{22}$/u.test(channel)) inputError();
  const id = youtubeVideoId(video);
  return { site: "onecrew-youtube", name: "language", args: ["-f", "json", "--", channel, id], addons: ["onecrew-youtube", "onecrew-browser-search"], maximumBytes: 64000, decode(value: unknown) {
    const row = parsed(z.array(z.record(z.string(), z.unknown())).length(1), value)[0]!;
    if (row.author_id !== channel || row.source_url !== `https://www.youtube.com/watch?v=${id}`) outputError();
    return row;
  } } satisfies ReviewedOpenCliRead<unknown>;
}
export function youtubeRecentLanguagePlan(channel: string) {
  if (!/^UC[A-Za-z\d_-]{22}$/u.test(channel)) inputError();
  return { site: "onecrew-youtube", name: "language", args: ["-f", "json", "--", channel, "recent"], addons: ["onecrew-youtube", "onecrew-browser-search"], maximumBytes: 64000, decode(value: unknown) {
    const rows = parsed(z.array(z.object({ source_url: text }).strict()).max(3), value);
    const seen = new Set<string>();
    return rows.map(row => {
      const url = youtubeUrl(row.source_url);
      const id = url?.searchParams.get("v") ?? "";
      if (!idValid(id) || row.source_url !== `https://www.youtube.com/watch?v=${id}` || seen.has(row.source_url)) outputError();
      seen.add(row.source_url); return row.source_url;
    });
  } } satisfies ReviewedOpenCliRead<unknown>;
}
