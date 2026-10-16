import { parse as parseJavaScript } from "acorn";
import type { VariableDeclaration } from "acorn";
import { load } from "cheerio";
import { parseStoredJson } from "../../contracts/stored-json.ts";
import { createReadSession, NativeReadTransportError } from "../../native-capabilities/network.ts";
import type { ReadCreatorProfile } from "../../creators/candidate-input.ts";
import type { CandidateEvidence } from "../../creators/candidate-contracts.ts";
import { CreatorProviderReadError, observedCount, record } from "./projection.ts";
import { YoutubeApiReadError, youtubeHttpFailure } from "../browser/youtube-health.ts";
import type { YoutubeRequest } from "../browser/youtube-request-budget.ts";

function text(value: unknown) { return typeof value === "string" && value.isWellFormed() ? value : ""; }
function findAbout(value: unknown, depth = 0): Record<string, unknown> | null {
  if (depth > 64) throw new CreatorProviderReadError("provider_read", "youtube_about_invalid");
  if (!value || typeof value !== "object") return null;
  if (!Array.isArray(value) && Object.hasOwn(value, "aboutChannelViewModel")) return record(record(value).aboutChannelViewModel);
  for (const item of Object.values(value)) { const found = findAbout(item, depth + 1); if (found) return found; } return null;
}
function aboutFromScripts(sources: readonly string[]): Record<string, unknown> {
  for (const source of sources) {
    if (!source.includes("ytInitialData")) continue;
    const program = parseJavaScript(source, { ecmaVersion: "latest" });
    for (const node of program.body) {
      if (node.type !== "VariableDeclaration") continue;
      for (const declaration of (node as VariableDeclaration).declarations) {
        if (declaration.id.type !== "Identifier" || declaration.id.name !== "ytInitialData" || declaration.init?.type !== "ObjectExpression") continue;
        const model = findAbout(parseStoredJson(source.slice(declaration.init.start, declaration.init.end)));
        if (model) return model;
      }
    }
  }
  throw new CreatorProviderReadError("provider_read", "youtube_about_missing");
}
export function parseYoutubeAbout(html: string): { description: string; links: string[] } {
  const $ = load(html); const model = aboutFromScripts($("script").toArray().map(element => $(element).text()));
  const links: string[] = [];
  if (Array.isArray(model.links)) for (const value of model.links) {
    const item = record(value); if (!item.channelExternalLinkViewModel) continue;
    const view = record(item.channelExternalLinkViewModel); if (!view.link) continue;
    const link = record(view.link); let destination: string | null = null;
    if (Array.isArray(link.commandRuns)) for (const entry of link.commandRuns) {
      const run = record(entry); if (!run.onTap) continue; const tap = record(run.onTap); if (!tap.innertubeCommand) continue;
      const command = record(tap.innertubeCommand); if (!command.urlEndpoint) continue;
      const url = URL.parse(text(record(command.urlEndpoint).url)); if (!url) continue;
      destination = url.hostname.replace(/^www\./u, "") === "youtube.com" && url.pathname === "/redirect" ? url.searchParams.get("q") : url.href;
      if (destination !== null) break;
    }
    if (destination === null) destination = text(link.content).trim() || null;
    if (destination === null) continue;
    if (!destination.includes("://") && !destination.startsWith("mailto:")) destination = `https://${destination.replace(/^\/+/, "")}`;
    const url = URL.parse(destination);
    if (!url || url.protocol !== "mailto:" && (!["http:", "https:"].includes(url.protocol) || !url.hostname.includes(".") || url.username || url.password)) continue;
    if (!links.includes(destination)) links.push(destination);
  }
  return { description: text(model.description).trim(), links };
}
export function parseNativeYoutubeAbout(html: string) {
  const $ = load(html), model = aboutFromScripts($("script").toArray().map(element => $(element).text()));
  const value = model.signInForBusinessEmail;
  const hidden = typeof value === "boolean" ? value : value == null ? false : typeof value === "string" ? value.trim().length > 0 : typeof value === "object" && !Array.isArray(value) ? Object.keys(value).length > 0 : true;
  return { ...parseYoutubeAbout(html), hidden_business_email: hidden };
}
export function projectYoutubeProfile(payload: unknown, selected: string, about: { description: string; links: string[] } | null, recent: unknown | null): ReadCreatorProfile {
  const response = record(payload); if (!Array.isArray(response.items) || !response.items.length) throw new CreatorProviderReadError("provider_read", "youtube_channel_missing");
  const channel = record(response.items[0]); const id = text(channel.id);
  if (id !== selected) throw new CreatorProviderReadError("identity_mismatch", "youtube_channel_identity_mismatch");
  const snippet = record(channel.snippet); const statistics = record(channel.statistics); const title = text(snippet.title);
  if (!title) throw new CreatorProviderReadError("provider_read", "youtube_channel_title_missing");
  const source = `https://www.youtube.com/channel/${id}`; const aboutUrl = `${source}/about`;
  const description = about?.description || text(snippet.description); const descriptionSource = about?.description ? aboutUrl : null;
  const evidence: CandidateEvidence[] = [{ kind: "profile_field", source: "channel_description", source_url: descriptionSource, text: description }];
  const country = text(snippet.country) || null;
  if (country !== null) evidence.push({ kind: "profile_field", source: "profile_location", source_url: source, text: country });
  if (recent !== null) {
    const page = record(recent); if (!Array.isArray(page.items)) throw new CreatorProviderReadError("provider_read", "youtube_recent_invalid");
    if (page.items.length > 5) throw new CreatorProviderReadError("provider_read", "youtube_recent_invalid");
    for (const value of page.items) { const item = record(value); const snippet = record(item.snippet); const resource = record(snippet.resourceId); const videoId = text(resource.videoId); if (videoId) evidence.push({ kind: "profile_field", source: "video_description", source_url: `https://www.youtube.com/watch?v=${videoId}`, text: text(snippet.description) }); }
  }
  const websites = about?.links ?? []; for (const target_url of websites) evidence.push({ kind: "profile_link", source: "profile_website", source_url: aboutUrl, display_text: target_url, target_url });
  const warnings: string[] = [];
  if (about === null) warnings.push("YouTube About information could not be read; this preview may omit public links and description evidence.");
  if (recent === null) warnings.push("YouTube recent videos could not be read; this preview omits their description evidence.");
  const thumbnails = snippet.thumbnails ? record(snippet.thumbnails) : {}; let avatar: string | null = null;
  for (const size of ["high", "medium", "default"]) { const value = thumbnails[size]; if (value && typeof value === "object") avatar = text(record(value).url) || null; if (avatar) break; }
  const rawCount = text(statistics.subscriberCount); const count = /^\d+$/u.test(rawCount) ? observedCount(BigInt(rawCount)) : null;
  const custom = text(snippet.customUrl).trim(); const handle = custom ? custom.replace(/^https:\/\/www\.youtube\.com\//u, "").replace(/^@+/u, "") : null;
  return { observed_at: Math.floor(Date.now() / 1000), account: { platform: "youtube", external_id: id, target_kind: "channel", handle, display_name: title, profile_url: source }, profile: { avatar_url: avatar, description, location: country, website_urls: websites, follower_count: count }, evidence, warnings, administrator: null };
}
export async function readYoutubeCreator(apiKey: string, channelId: string, signal: AbortSignal, assertCurrent: () => Promise<void>, request: YoutubeRequest): Promise<ReadCreatorProfile> {
  if (!/^UC[A-Za-z\d_-]{22}$/u.test(channelId)) throw new CreatorProviderReadError("source_mismatch", "youtube_channel_target_invalid");
  const session = await createReadSession(["https://www.googleapis.com", "https://www.youtube.com"], undefined, { browser: "chrome_149", headers: {}, classifyHttpFailure: (status, body, url) => url.origin === "https://www.googleapis.com" ? youtubeHttpFailure(status, body) : new NativeReadTransportError("http_rejected", status), allowPath: url => url.origin === "https://www.googleapis.com" ? ["/youtube/v3/channels", "/youtube/v3/playlistItems"].includes(url.pathname) : url.pathname === `/channel/${channelId}/about` });
  const api = async (kind: "channels" | "playlistItems", params: Record<string, string>) => {
    const url = new URL(`https://www.googleapis.com/youtube/v3/${kind}`); for (const [key, value] of Object.entries(params)) url.searchParams.set(key, value); url.searchParams.set("key", apiKey);
    await assertCurrent(); const payload = await request(async signal => { await assertCurrent(); return session.getJson(url, signal); }, signal).then(value => value, error => { signal.throwIfAborted(); if (error instanceof NativeReadTransportError) throw new YoutubeApiReadError(error.message === "native_read_request_failed" || error.message === "native_read_body_unavailable" ? "transient" : "infrastructure"); throw error; }); await assertCurrent(); return payload;
  };
  try {
    const payload = await api("channels", { part: "snippet,statistics,contentDetails", id: channelId });
    const envelope = record(payload); if (!Array.isArray(envelope.items) || !envelope.items.length) throw new CreatorProviderReadError("provider_read", "youtube_channel_missing");
    const channel = record(envelope.items[0]); if (channel.id !== channelId) throw new CreatorProviderReadError("identity_mismatch", "youtube_channel_identity_mismatch");
    const details = channel.contentDetails ? record(channel.contentDetails) : {}; const playlists = details.relatedPlaylists ? record(details.relatedPlaylists) : {}; const uploads = text(playlists.uploads);
    const recent = uploads ? api("playlistItems", { part: "snippet", playlistId: uploads, maxResults: "5" }).then(value => value, error => { if (error instanceof CreatorProviderReadError || error instanceof YoutubeApiReadError) return null; throw error; }) : Promise.resolve({ items: [] });
    const about = (async () => { await assertCurrent(); const html = await session.getText(new URL(`https://www.youtube.com/channel/${channelId}/about`), signal); await assertCurrent(); return parseYoutubeAbout(html); })().then(value => value, error => { signal.throwIfAborted(); if (error instanceof NativeReadTransportError || error instanceof CreatorProviderReadError || error instanceof SyntaxError) return null; throw error; });
    const [activity, page] = await Promise.all([recent, about]);
    return projectYoutubeProfile(payload, channelId, page, activity);
  } finally { await session.close(); }
}
