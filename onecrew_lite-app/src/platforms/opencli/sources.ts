import { OpenCliError } from "./errors.ts";
import { validateOpenCliProfile } from "./registry.ts";
import type { OpenCliAddonId } from "./addons.ts";

interface SearchSource { readonly site: string; readonly name: string; readonly maximum: number; readonly addon: OpenCliAddonId | null; readonly discovery: boolean }
export const CREATOR_SEARCH_SOURCES: Readonly<Record<string, SearchSource>> = Object.freeze({
  youtube: { site: "onecrew-search-youtube", name: "search", maximum: 50, addon: "onecrew-browser-search", discovery: true },
  tiktok: { site: "onecrew-tiktok", name: "search", maximum: 50, addon: "onecrew-tiktok", discovery: true },
  instagram: { site: "onecrew-search-instagram", name: "search", maximum: 50, addon: "onecrew-browser-search", discovery: true },
  twitter: { site: "onecrew-search-twitter", name: "search", maximum: 50, addon: "onecrew-browser-search", discovery: true },
  linkedin: { site: "onecrew-search-linkedin", name: "people-search", maximum: 10, addon: "onecrew-browser-search", discovery: true },
  facebook: { site: "onecrew-search-facebook", name: "search", maximum: 50, addon: "onecrew-browser-search", discovery: true },
  pinterest: { site: "onecrew-pinterest-reader", name: "search-pins", maximum: 20, addon: "onecrew-browser-search", discovery: true },
  reddit: { site: "onecrew-search-reddit", name: "search", maximum: 50, addon: "onecrew-browser-search", discovery: true },
  bilibili: { site: "onecrew-bilibili", name: "search", maximum: 50, addon: "onecrew-bilibili", discovery: true },
  douyin: { site: "onecrew-search-douyin", name: "search", maximum: 30, addon: "onecrew-browser-search", discovery: true },
  weibo: { site: "onecrew-search-weibo", name: "search", maximum: 50, addon: "onecrew-browser-search", discovery: true },
  google: { site: "onecrew-search-google", name: "search", maximum: 50, addon: "onecrew-browser-search", discovery: true },
  yahoo: { site: "onecrew-search-yahoo", name: "search", maximum: 7, addon: "onecrew-browser-search", discovery: true },
  tieba: { site: "onecrew-search-tieba", name: "search", maximum: 20, addon: "onecrew-browser-search", discovery: false },
});
for (const source of Object.values(CREATOR_SEARCH_SOURCES)) Object.freeze(source);

export const RETIRED_CREATOR_SOURCES = Object.freeze(["zhihu", "pixiv", "weixin", "stackoverflow", "stackoverflow-users", "devto-tag", "hf-spaces", "nowcoder", "producthunt-posts", "medium", "medium-tag", "bluesky", "jike", "dribbble-designers", "hupu", "apple-podcasts", "pinterest-pins", "pinterest-boards", "dribbble-shots", "google-images", "reddit-subreddit", "linkedin-learning", "substack", "v2ex-node", "lesswrong-tag", "lobsters-tag", "stackoverflow-tag", "juejin-hot", "linux-do", "linkedin-salesnav", "duckduckgo", "archive", "weread", "douban-books", "douban-music", "hf-models", "hf-datasets", "hackernews"]);

export function creatorSearchSource(platform: string): SearchSource {
  const source = Object.hasOwn(CREATOR_SEARCH_SOURCES, platform) ? CREATOR_SEARCH_SOURCES[platform] : undefined;
  if (!source) throw new OpenCliError("opencli_read_command_unavailable");
  return source;
}
export function creatorSearchArguments(profile: string, platform: string, query: string, limit: number): readonly string[] {
  const source = creatorSearchSource(platform);
  validateOpenCliProfile(profile);
  if (!query.isWellFormed() || !query.trim() || [...query].length > 160 || /\p{Cc}/u.test(query) || !Number.isInteger(limit) || limit < 1 || limit > source.maximum) throw new OpenCliError("opencli_invalid_input");
  return Object.freeze(["--profile", profile, source.site, source.name, "--limit", String(limit), "-f", "json", "--", query]);
}
export function ambiguousCreatorSearchEmpty(platform: string): boolean {
  return ["facebook", "linkedin", "tieba", "google", "yahoo"].includes(platform);
}
