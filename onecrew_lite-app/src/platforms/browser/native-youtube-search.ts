import { z } from "zod";
import { createReadSession, NativeReadTransportError } from "../../native-capabilities/network.ts";
import { youtubeSearchInput } from "../../discovery/native-search-inputs.ts";
import { YoutubeApiReadError, youtubeHttpFailure } from "./youtube-health.ts";
import type { YoutubeRequest } from "./youtube-request-budget.ts";

const text = z.string().refine(value => value.isWellFormed()), string = text.default("");
const snippet = z.object({ channelId: string, title: string, description: string, publishedAt: string }).default({ channelId: "", title: "", description: "", publishedAt: "" });
const searchResponse = z.object({ items: z.array(z.object({ id: z.object({ channelId: string, videoId: string }).default({ channelId: "", videoId: "" }), snippet })).default([]), nextPageToken: text.nullish().transform(value => value ?? null) });
const image = z.object({ url: string }).default({ url: "" });
const channelResponse = z.object({ items: z.array(z.object({ id: text, snippet: z.object({ title: string, description: string, customUrl: string, country: string, thumbnails: z.object({ high: image, medium: image, default: image }).default({ high: { url: "" }, medium: { url: "" }, default: { url: "" } }) }).default({ title: "", description: "", customUrl: "", country: "", thumbnails: { high: { url: "" }, medium: { url: "" }, default: { url: "" } } }), statistics: z.object({ subscriberCount: string, viewCount: string, videoCount: string }).default({ subscriberCount: "", viewCount: "", videoCount: "" }) })).default([]) });
const nonempty = (value: string) => value.trim() ? value : null;
function count(value: string): bigint | null {
  if (!/^\+?\d+$/u.test(value)) return null;
  const parsed = BigInt(value); return parsed <= 18446744073709551615n ? parsed : null;
}
export function projectNativeYoutubeSearch(search: unknown, metadata: unknown) {
  const page = searchResponse.parse(search), channels: Record<string, { title: string; thumbnail_url: string | null; description: string; custom_url: string | null; country: string | null; subscriber_count: bigint | null; view_count: bigint | null; video_count: bigint | null }> = {};
  const items = page.items.flatMap(item => {
    const channelId = nonempty(item.id.channelId) ?? nonempty(item.snippet.channelId);
    return channelId === null ? [] : [{ channel_id: channelId, channel_url: `https://www.youtube.com/channel/${channelId}`, matched: { title: item.snippet.title, description: item.snippet.description, video_id: nonempty(item.id.videoId), published_at: nonempty(item.snippet.publishedAt) } }];
  });
  for (const channel of channelResponse.parse(metadata).items.sort((a, b) => Buffer.compare(Buffer.from(a.id), Buffer.from(b.id)))) {
    const row = channel.snippet; Object.defineProperty(channels, channel.id, { enumerable: true, configurable: true, writable: true, value: { title: row.title, thumbnail_url: [row.thumbnails.high.url, row.thumbnails.medium.url, row.thumbnails.default.url].find(value => value.length > 0) ?? null, description: row.description, custom_url: nonempty(row.customUrl), country: nonempty(row.country), subscriber_count: count(channel.statistics.subscriberCount), view_count: count(channel.statistics.viewCount), video_count: count(channel.statistics.videoCount) } });
  }
  return { items, channels, next_page_token: page.nextPageToken };
}

export async function readNativeYoutubeSearch(apiKey: string, kind: "channel" | "video", arguments_: unknown, signal: AbortSignal, assertCurrent: () => Promise<void>, request: YoutubeRequest) {
  const input = youtubeSearchInput.parse(arguments_);
  const session = await createReadSession(["https://www.googleapis.com"], undefined, { browser: "chrome_149", headers: {}, allowPath: url => ["/youtube/v3/search", "/youtube/v3/channels"].includes(url.pathname), classifyHttpFailure: youtubeHttpFailure });
  async function get(resource: "search" | "channels", query: Record<string, string>) {
    const url = new URL(`https://www.googleapis.com/youtube/v3/${resource}`); for (const [key, value] of Object.entries(query)) url.searchParams.set(key, value); url.searchParams.set("key", apiKey);
    await assertCurrent(); const value = await request(async signal => { await assertCurrent(); return session.getJson(url, signal); }, signal).then(value => value, error => { signal.throwIfAborted(); if (error instanceof NativeReadTransportError) throw new YoutubeApiReadError(error.status === null ? "transient" : "infrastructure"); throw error; }); await assertCurrent(); return value;
  }
  try {
    const query: Record<string, string> = { part: "snippet", q: input.query, type: kind, maxResults: String(input.count), order: input.order };
    for (const [key, value] of [["publishedAfter", input.published_after], ["regionCode", input.region_code], ["relevanceLanguage", input.language], ["pageToken", input.cursor]] as const) if (value !== null && value.trim()) query[key] = value;
    const search = await get("search", query), projected = projectNativeYoutubeSearch(search, {});
    if (projected.items.length === 0) return projected;
    const channelIds = [...new Set(projected.items.map(item => item.channel_id))].sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)));
    const metadata = await get("channels", { part: "snippet,statistics", id: channelIds.join(",") });
    return projectNativeYoutubeSearch(search, metadata);
  } finally { await session.close(); }
}
