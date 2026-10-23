import { z } from "zod";
import { createReadSession, NativeReadTransportError } from "../native-capabilities/network.ts";
import { parseNativeYoutubeAbout } from "./candidates/youtube-read.ts";
import { CreatorProviderReadError } from "./candidates/projection.ts";
import { YoutubeApiReadError, youtubeHttpFailure } from "./browser/youtube-health.ts";
import type { YoutubeRequest } from "./browser/youtube-request-budget.ts";

const text = z.string().refine(value => value.isWellFormed()), string = text.default("");
const image = z.object({ url: string }).default({ url: "" });
const channelSchema = z.object({ id: text, snippet: z.object({ title: string, description: string, customUrl: string, country: string, thumbnails: z.object({ high: image, medium: image, default: image }).default({ high: { url: "" }, medium: { url: "" }, default: { url: "" } }) }).default({ title: "", description: "", customUrl: "", country: "", thumbnails: { high: { url: "" }, medium: { url: "" }, default: { url: "" } } }), statistics: z.object({ subscriberCount: string, viewCount: string, videoCount: string }).default({ subscriberCount: "", viewCount: "", videoCount: "" }), contentDetails: z.object({ relatedPlaylists: z.object({ uploads: string }).default({ uploads: "" }) }).default({ relatedPlaylists: { uploads: "" } }) });
const channelPage = z.object({ items: channelSchema.array().default([]) });
export const nativeYoutubeChannelSchema = channelSchema;
const recentPage = z.object({ items: z.array(z.object({ snippet: z.object({ title: string, description: string, publishedAt: string, resourceId: z.object({ videoId: string }).default({ videoId: "" }) }) })).default([]) });
const commentsPage = z.object({ items: z.array(z.object({ snippet: z.object({ topLevelComment: z.object({ id: string, snippet: z.object({ authorDisplayName: string, authorChannelId: z.object({ value: string }).default({ value: "" }), textDisplay: string, publishedAt: string }) }) }) })).default([]) });
type About = ReturnType<typeof parseNativeYoutubeAbout>;
type Recent = z.infer<typeof recentPage>;
type Comments = z.infer<typeof commentsPage>;
const nonempty = (value: string) => value.trim().length ? value : null;
function count(value: string): bigint | null { if (!/^\+?\d+$/u.test(value)) return null; const number = BigInt(value); return number <= 18446744073709551615n ? number : null; }
export function projectNativeYoutubeChannel(channel: z.infer<typeof channelSchema>, about: About | null, recent: Recent | null, comments: Comments | null, depth: "light" | "full") {
  const channelUrl = `https://www.youtube.com/channel/${channel.id}`, aboutUrl = `${channelUrl}/about`, row = channel.snippet;
  const unavailable: string[] = [], omitted = depth === "light" ? ["public_comments"] : [];
  if (recent === null) unavailable.push("recent_videos");
  const videos = (recent?.items ?? []).flatMap(item => nonempty(item.snippet.resourceId.videoId) === null ? [] : [{ video_id: item.snippet.resourceId.videoId, title: item.snippet.title, description: item.snippet.description, published_at: nonempty(item.snippet.publishedAt) }]);
  if (depth === "full" && videos.length > 0 && comments === null) unavailable.push("public_comments");
  if (about === null) unavailable.push("about");
  return { channel_id: channel.id, channel_url: channelUrl, title: row.title, thumbnail_url: [row.thumbnails.high.url, row.thumbnails.medium.url, row.thumbnails.default.url].find(value => value.length > 0) ?? null,
    description: about?.description || row.description, description_source_url: about?.description ? aboutUrl : null, custom_url: nonempty(row.customUrl), country: nonempty(row.country), subscriber_count: count(channel.statistics.subscriberCount), view_count: count(channel.statistics.viewCount), video_count: count(channel.statistics.videoCount), matched: null,
    recent_videos: videos, public_comments: (comments?.items ?? []).map(item => { const comment = item.snippet.topLevelComment; return { comment_id: comment.id, author_name: comment.snippet.authorDisplayName, author_channel_id: nonempty(comment.snippet.authorChannelId.value), text: comment.snippet.textDisplay, published_at: nonempty(comment.snippet.publishedAt) }; }),
    about_links: (about?.links ?? []).map(target_url => ({ target_url, source_url: aboutUrl })), hidden_business_email: about?.hidden_business_email ?? false, unavailable_sections: unavailable, ...(omitted.length ? { omitted_sections: omitted } : {}) };
}
export async function readNativeYoutubeChannel(apiKey: string, input: { channel_id: string; depth: "light" | "full" }, signal: AbortSignal, assertCurrent: () => Promise<void>, request: YoutubeRequest) {
  if (!/^UC[A-Za-z0-9_-]{22}$/u.test(input.channel_id) || !["light", "full"].includes(input.depth)) throw new Error("youtube.channel_input_invalid");
  const session = await createReadSession(["https://www.googleapis.com"], undefined, { browser: "chrome_149", headers: {}, allowPath: url => ["/youtube/v3/channels", "/youtube/v3/playlistItems", "/youtube/v3/commentThreads"].includes(url.pathname), classifyHttpFailure: youtubeHttpFailure });
  async function get(resource: string, values: Record<string, string>) {
    const url = new URL(`https://www.googleapis.com/youtube/v3/${resource}`); for (const [key, value] of Object.entries(values)) url.searchParams.set(key, value); url.searchParams.set("key", apiKey);
    await assertCurrent(); const result = await request(async signal => { await assertCurrent(); return session.getJson(url, signal); }, signal); await assertCurrent(); return result;
  }
  async function section<T>(operation: () => Promise<T>): Promise<T | null> {
    return operation().then(value => value, error => { signal.throwIfAborted(); if (error instanceof NativeReadTransportError || error instanceof YoutubeApiReadError || error instanceof CreatorProviderReadError || error instanceof z.ZodError || error instanceof SyntaxError) return null; throw error; });
  }
  try {
    const channel = channelPage.parse(await get("channels", { part: "snippet,statistics,contentDetails", id: input.channel_id })).items[0];
    if (!channel) throw new Error("youtube.channel_not_found");
    if (channel.id !== input.channel_id) throw new Error("youtube.channel_identity_mismatch");
    const uploads = channel.contentDetails.relatedPlaylists.uploads;
    const activity = async () => {
      const recent = !uploads ? { items: [] } : await section(async () => recentPage.parse(await get("playlistItems", { part: "snippet", playlistId: uploads, maxResults: "5" })));
      if (recent && recent.items.length > 5) throw new Error("youtube.recent_page_invalid");
      const video = recent?.items.find(item => nonempty(item.snippet.resourceId.videoId) !== null);
      const comments = input.depth === "light" || !video ? { items: [] } : await section(async () => commentsPage.parse(await get("commentThreads", { part: "snippet", videoId: video.snippet.resourceId.videoId, maxResults: "10", textFormat: "plainText", order: "relevance" })));
      if (comments && comments.items.length > 10) throw new Error("youtube.comments_page_invalid");
      return { recent, comments };
    };
    const about = async () => section(async () => {
      const url = new URL(`https://www.youtube.com/channel/${channel.id}/about`), publicSession = await createReadSession([url.origin], undefined, { browser: "chrome_149", headers: {}, allowPath: selected => selected.pathname === url.pathname });
      try { await assertCurrent(); const body = await publicSession.getText(url, signal); await assertCurrent(); return parseNativeYoutubeAbout(body); } finally { await publicSession.close(); }
    });
    const results = await Promise.allSettled([activity(), about()]);
    const [activityResult, aboutResult] = results;
    if (activityResult!.status === "rejected") throw activityResult.reason;
    if (aboutResult!.status === "rejected") throw aboutResult.reason;
    await assertCurrent(); return projectNativeYoutubeChannel(channel, aboutResult!.value, activityResult!.value.recent, activityResult!.value.comments, input.depth);
  } finally { await session.close(); }
}
