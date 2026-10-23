import { createSession, RequestError } from "wreq-js";
import { parse as parseJavaScript } from "acorn";
import { load } from "cheerio";
import { advancedXSearchInput, compileAdvancedXQuery } from "../../discovery/native-search-inputs.ts";
import { parseStoredJson } from "../../contracts/stored-json.ts";
import { parseApplicationJson, stringifyApplicationJson } from "../../contracts/application-json.ts";
import { xReadHeaders, nativeProxy } from "../candidates/social-read.ts";
import type { PlatformProfileSecret } from "./platform-vault.ts";
import { projectNativeXTimeline } from "./native-x-projection.ts";

const queryIds = { SearchTimeline: "VhUd6vHVmLBcw0uX-6jMLA", UserByScreenName: "1VOOyvKkiI3FMmkeDNxM9A", UserTweets: "q6xj5bs0hapm9309hexA_g", TweetDetail: "xd_EMdYvB9hfZsZ6Idri0w" } as const;
const publicIdSource = "https://raw.githubusercontent.com/fa0311/twitter-openapi/refs/heads/main/src/config/placeholder.json";
const features = Object.fromEntries(["responsive_web_graphql_exclude_directive_enabled", "creator_subscriptions_tweet_preview_api_enabled", "responsive_web_graphql_timeline_navigation_enabled", "c9s_tweet_anatomy_moderator_badge_enabled", "tweetypie_unmention_optimization_enabled", "responsive_web_edit_tweet_api_enabled", "graphql_is_translatable_rweb_tweet_is_translatable_enabled", "view_counts_everywhere_api_enabled", "longform_notetweets_consumption_enabled", "responsive_web_twitter_article_tweet_consumption_enabled", "longform_notetweets_rich_text_read_enabled", "longform_notetweets_inline_media_enabled", "rweb_video_timestamps_enabled", "responsive_web_media_download_video_enabled", "freedom_of_speech_not_reach_fetch_enabled", "standardized_nudges_misinfo"].map(value => [value, true]));
const record = (value: unknown): Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
export class XReadFailure extends Error {
  readonly status: number | null;
  readonly providerCode: number | null;
  constructor(status: number | null, code: number | null = null) { super("x.provider_read_failed"); this.status = status; this.providerCode = code; }
}
export function findSearchQueryIdInJavaScript(source: string, operation: string = "SearchTimeline"): string | null {
  const root = parseJavaScript(source, { ecmaVersion: "latest", sourceType: "script" }); let found: string | null = null;
  function visit(value: unknown): void {
    if (found || value === null || typeof value !== "object") return;
    if (Array.isArray(value)) { for (const item of value) visit(item); return; }
    const node = record(value);
    if (node.type === "ObjectExpression") {
      const values: Record<string, unknown> = {};
      for (const item of Array.isArray(node.properties) ? node.properties : []) {
        const property = record(item), key = record(property.key), literal = record(property.value);
        if (property.type === "Property" && !property.computed && literal.type === "Literal") { const name = key.type === "Identifier" ? key.name : key.value; if (typeof name === "string") Object.defineProperty(values, name, { configurable: true, enumerable: true, value: literal.value }); }
      }
      if (values.operationName === operation && typeof values.queryId === "string" && /^[A-Za-z0-9_-]+$/u.test(values.queryId)) found = values.queryId;
    }
    for (const child of Object.values(node)) visit(child);
  }
  visit(root); return found;
}
export async function readNativeXAdvancedSearch(secret: PlatformProfileSecret, arguments_: unknown, signal: AbortSignal, assertCurrent: () => Promise<void>) {
  const input = advancedXSearchInput.parse(arguments_), effectiveQuery = compileAdvancedXQuery(input);
  const variables = { rawQuery: effectiveQuery, count: input.count, querySource: "typed_query", product: input.result_mode === "latest" ? "Latest" : "Top", ...(input.cursor === null ? {} : { cursor: input.cursor }) };
  return { effective_query: effectiveQuery, page: projectNativeXTimeline(await readNativeXGraphql(secret, "SearchTimeline", variables, "POST", signal, assertCurrent)) };
}
export async function readNativeXGraphql(secret: PlatformProfileSecret, operation: keyof typeof queryIds, variables: Readonly<Record<string, unknown>>, method: "GET" | "POST", signal: AbortSignal, assertCurrent: () => Promise<void>): Promise<unknown> {
  if (!Object.hasOwn(queryIds, operation)) throw new Error("x.read_operation_invalid");
  const proxy = nativeProxy(secret.proxy);
  const session = await createSession({ browser: "chrome_149", os: process.platform === "win32" ? "windows" : "macos", ...(proxy ? { proxy } : {}) });
  let publicSession: Awaited<ReturnType<typeof createSession>> | undefined;
  async function fetchText(url: string, authenticated: boolean, body?: string) {
    const selected = new URL(url);
    if (authenticated ? selected.origin !== "https://x.com" : !["https://raw.githubusercontent.com", "https://abs.twimg.com"].includes(selected.origin)) throw new Error("x.transport_origin_refused");
    await assertCurrent(); signal.throwIfAborted();
    const transport = authenticated ? session : publicSession; if (!transport) throw new Error("x.public_transport_unavailable");
    const response = await transport.fetch(url, { method: body === undefined ? "GET" : "POST", redirect: "error", signal: AbortSignal.any([signal, AbortSignal.timeout(30_000)]), ...(authenticated ? { headers: { ...xReadHeaders(secret), ...(body === undefined ? {} : { "content-type": "application/json", referer: "https://x.com/compose/post" }) } } : {}), ...(body === undefined ? {} : { body }) }).then(value => value, error => { signal.throwIfAborted(); if (error instanceof RequestError) throw new XReadFailure(null); throw error; });
    if (!response.body) throw new XReadFailure(response.status);
    const reader = response.body.getReader(), chunks: Uint8Array[] = []; let bytes = 0;
    try { for (;;) { const item = await reader.read(); if (item.done) break; bytes += item.value.length; if (bytes > 4 * 1024 * 1024) throw new Error("x.response_too_large"); chunks.push(item.value); } } finally { await reader.cancel(); reader.releaseLock(); }
    await assertCurrent(); signal.throwIfAborted();
    return { status: response.status, ok: response.ok, text: new TextDecoder("utf-8", { fatal: true }).decode(Buffer.concat(chunks)) };
  }
  async function execute(queryId: string) {
    const url = new URL(`https://x.com/i/api/graphql/${queryId}/${operation}`);
    if (method === "GET") { url.searchParams.set("variables", stringifyApplicationJson(variables)); url.searchParams.set("features", stringifyApplicationJson(features)); }
    const response = await fetchText(url.href, true, method === "POST" ? stringifyApplicationJson({ variables, queryId, features }) : undefined);
    const payload = await Promise.resolve().then(() => parseStoredJson(response.text)).then(value => value, error => { if (!response.ok && error instanceof SyntaxError) return {}; throw error; });
    const envelope = record(payload), topError = Array.isArray(envelope.errors) ? envelope.errors[0] : undefined;
    const nestedError = Object.values(record(envelope.data)).map(item => record(item).errors).find(item => Array.isArray(item) && item.length > 0);
    if (!response.ok || topError !== undefined || nestedError !== undefined) {
      const error = record(topError ?? (Array.isArray(nestedError) ? nestedError[0] : undefined)), code = Number(record(error.code).value ?? error.code);
      throw new XReadFailure(response.status, Number.isSafeInteger(code) ? code : null);
    }
    return payload;
  }
  try {
    publicSession = await createSession({ browser: "chrome_149", os: process.platform === "win32" ? "windows" : "macos", ...(proxy ? { proxy } : {}) });
    const initial = await execute(queryIds[operation]).then(value => ({ value }), error => ({ error }));
    if ("value" in initial) return initial.value;
    if (!(initial.error instanceof XReadFailure) || initial.error.status === null || ![404, 422].includes(initial.error.status)) throw initial.error;
    const ids = await fetchText(publicIdSource, false).then(response => response.ok ? parseApplicationJson(response.text) : null, () => null);
    const publicId = record(record(ids)[operation]).queryId;
    if (typeof publicId === "string" && /^[A-Za-z0-9_-]+$/u.test(publicId)) return execute(publicId);
    const home = await fetchText("https://x.com/", true); if (!home.ok) throw new XReadFailure(home.status);
    const $ = load(home.text), urls = [...new Set($("script[src],link[href]").toArray().map(item => $(item).attr("src") ?? $(item).attr("href") ?? "").filter(value => value.startsWith("https://abs.twimg.com/responsive-web/client-web") && value.endsWith(".js")))].slice(0, 40);
    for (const url of urls) {
      const response = await fetchText(url, false); if (!response.ok) throw new XReadFailure(response.status);
      const id = findSearchQueryIdInJavaScript(response.text, operation); if (id) return execute(id);
    }
    throw new Error("x.search_operation_unavailable");
  } finally { await Promise.all([session.close(), ...(publicSession ? [publicSession.close()] : [])]); }
}
