import { randomInt, randomUUID } from "node:crypto";
import { z } from "zod";
import { parseApplicationJson } from "../../contracts/application-json.ts";
import { createReadSession, NativeReadTransportError } from "../../native-capabilities/network.ts";
import type { PlatformProfileSecret } from "../browser/platform-vault.ts";
import { CreatorProviderReadError, projectXProfile, projectInstagramProfile } from "./projection.ts";

const proxySchema = z.object({ kind: z.enum(["http", "https", "socks5"]), host: z.string().min(1), port: z.number().int().min(1).max(65535), username: z.string().nullable().optional(), password: z.string().nullable().optional() });
export function nativeProxy(value: unknown): string | undefined {
  if (value == null) return undefined;
  const proxy = proxySchema.parse(value); const url = URL.parse(`${proxy.kind}://${proxy.host}:${proxy.port}`);
  if (!url || !url.hostname || url.pathname && url.pathname !== "/" || url.search || url.hash || url.username || url.password) throw new Error("Platform proxy is invalid");
  if (proxy.username) { url.username = proxy.username; url.password = proxy.password ?? ""; }
  return url.href.replace(/\/$/u, "");
}
export function xReadHeaders(secret: PlatformProfileSecret): Readonly<Record<string, string>> {
  const cookies = new Map(secret.cookies.map(cookie => [cookie.name, cookie.value]));
  if (!cookies.get("auth_token") || !cookies.get("ct0")) throw new CreatorProviderReadError("provider_read", "x_session_missing");
  return Object.freeze({ authorization: "Bearer AAAAAAAAAAAAAAAAAAAAANRILgAAAAAAnNwIzUejRCOuH5E6I8xnZz4puTs%3D1Zv7ttfk8LF81IUq16cHjhLTvJu4FA33AGWWjCpTnA", cookie: [...cookies].sort(([a], [b]) => a.localeCompare(b, "en")).map(([name, value]) => `${name}=${value}`).join("; "), "x-csrf-token": cookies.get("ct0")!, "x-twitter-active-user": "yes", "x-twitter-auth-type": "OAuth2Session", "x-twitter-client-language": "en", accept: "*/*", origin: "https://x.com", referer: "https://x.com/" });
}
const xFeatures = Object.freeze(Object.fromEntries(["responsive_web_graphql_exclude_directive_enabled", "creator_subscriptions_tweet_preview_api_enabled", "responsive_web_graphql_timeline_navigation_enabled", "c9s_tweet_anatomy_moderator_badge_enabled", "tweetypie_unmention_optimization_enabled", "responsive_web_edit_tweet_api_enabled", "graphql_is_translatable_rweb_tweet_is_translatable_enabled", "view_counts_everywhere_api_enabled", "longform_notetweets_consumption_enabled", "responsive_web_twitter_article_tweet_consumption_enabled", "longform_notetweets_rich_text_read_enabled", "longform_notetweets_inline_media_enabled", "rweb_video_timestamps_enabled", "responsive_web_media_download_video_enabled", "freedom_of_speech_not_reach_fetch_enabled", "standardized_nudges_misinfo"].map(name => [name, true])));
const xPath = "/i/api/graphql/1VOOyvKkiI3FMmkeDNxM9A/UserByScreenName";
export function xProfileReadUrl(handle: string): URL {
  if (!/^[A-Za-z\d_]{1,15}$/u.test(handle)) throw new CreatorProviderReadError("source_mismatch", "x_profile_target_invalid");
  const url = new URL(`https://x.com${xPath}`); url.searchParams.set("variables", JSON.stringify({ screen_name: handle, withSafetyModeUserFields: true })); url.searchParams.set("features", JSON.stringify(xFeatures)); return url;
}
async function readFailure<T>(operation: Promise<T>, signal: AbortSignal, code: string): Promise<T> {
  return operation.then(value => value, error => { signal.throwIfAborted(); if (error instanceof NativeReadTransportError) throw new CreatorProviderReadError("provider_read", code); throw error; });
}
export async function readXCreator(secret: PlatformProfileSecret, handle: string, userId: string, signal: AbortSignal, assertCurrent: () => Promise<void>) {
  const url = xProfileReadUrl(handle); const session = await createReadSession(["https://x.com"], nativeProxy(secret.proxy), { browser: "chrome_149", headers: xReadHeaders(secret), allowPath: url => url.pathname === xPath });
  try { await assertCurrent(); const payload = await readFailure(session.getJson(url, signal), signal, "x_profile_read"); await assertCurrent(); return projectXProfile(payload, userId); } finally { await session.close(); }
}
const text = z.string().refine(value => value.isWellFormed() && !/\p{Cc}/u.test(value));
const igSessionSchema = z.object({ uuids: z.object({ uuid: text.min(1), phone_id: text.min(1), android_device_id: text.min(1) }), user_agent: text.min(1), cookies: z.record(z.string(), text).default({}), authorization_data: z.object({ ds_user_id: text.min(1), sessionid: text.min(1), should_use_header_over_cookies: z.boolean() }), mid: text.nullable().optional(), ig_www_claim: text.nullable().optional(), ig_u_rur: text.nullable().optional(), country: text.default("US"), locale: text.default("en_US"), timezone_offset: z.number().int().safe().default(0) });
export function instagramReadHeaders(secret: PlatformProfileSecret): Readonly<Record<string, string>> {
  if (!secret.instagram_session_settings) throw new CreatorProviderReadError("provider_read", "instagram_session_missing");
  const session = igSessionSchema.extend({ timezone_offset: z.union([z.number().int().safe(), z.bigint().min(-9223372036854775808n).max(9223372036854775807n)]).default(0) }).parse(parseApplicationJson(secret.instagram_session_settings));
  const locale = session.locale; const language = locale.replace(/_/gu, "-"); const id = session.authorization_data.ds_user_id;
  if (id !== secret.provider_account_id) throw new CreatorProviderReadError("identity_mismatch", "instagram_operator_identity_mismatch");
  return Object.freeze({ "x-ig-app-locale": locale, "x-ig-device-locale": locale, "x-ig-mapped-locale": locale, "x-pigeon-session-id": `UFS-${randomUUID()}-1`, "x-pigeon-rawclienttime": (Date.now() / 1000).toFixed(3), "x-ig-bandwidth-speed-kbps": (randomInt(2500000, 3000000) / 1000).toFixed(3), "x-ig-bandwidth-totalbytes-b": String(randomInt(5000000, 90000000)), "x-ig-bandwidth-totaltime-ms": String(randomInt(2000, 9000)), "x-ig-app-startup-country": session.country.toUpperCase(), "x-bloks-version-id": "7189b949425f9bf80ea8bd880cf5a3080b292d9b1c4b38a18d112f7c4b71e7a8", "x-ig-www-claim": session.ig_www_claim ?? "0", "x-bloks-is-layout-rtl": "false", "x-bloks-is-panorama-enabled": "true", "x-ig-device-id": session.uuids.uuid, "x-ig-family-device-id": session.uuids.phone_id, "x-ig-android-id": session.uuids.android_device_id, "x-ig-timezone-offset": String(session.timezone_offset), "x-ig-connection-type": "WIFI", "x-ig-capabilities": "3brTv10=", "x-ig-app-id": "567067343352427", priority: "u=3", "user-agent": session.user_agent, "accept-language": language === "en-US" ? "en-US" : `${language}, en-US`, "ig-intended-user-id": id, "x-ig-salt-ids": String(randomInt(1061162222, 1061262222)), "ig-u-ds-user-id": id, "x-ig-nav-chain": "9MV:self_profile:2,ProfileMediaTabFragment:self_profile:3,9Xf:self_following:4", "x-fb-http-engine": "Tigon/MNS/TCP", "x-tigon-is-retry": "False", "x-fb-client-ip": "True", "x-fb-server-cluster": "True", authorization: `Bearer IGT:2:${Buffer.from(JSON.stringify(session.authorization_data)).toString("base64")}`, ...(session.mid ? { "x-mid": session.mid } : {}), ...(session.ig_u_rur ? { "ig-u-rur": session.ig_u_rur } : {}), ...(Object.keys(session.cookies).length ? { cookie: Object.entries(session.cookies).sort(([a], [b]) => a.localeCompare(b, "en")).map(([name, value]) => `${name}=${value}`).join("; ") } : {}) });
}
export async function readInstagramCreator(secret: PlatformProfileSecret, handle: string, userId: string, signal: AbortSignal, assertCurrent: () => Promise<void>) {
  if (!/^[A-Za-z\d_.]{1,30}$/u.test(handle)) throw new CreatorProviderReadError("source_mismatch", "instagram_profile_target_invalid");
  const path = `/api/v1/users/${handle}/usernameinfo/`; const session = await createReadSession(["https://i.instagram.com"], nativeProxy(secret.proxy), { browser: "okhttp_5", headers: instagramReadHeaders(secret), allowPath: url => url.pathname === path });
  try { await assertCurrent(); const payload = await readFailure(session.getJson(new URL(`https://i.instagram.com${path}`), signal), signal, "instagram_profile_read"); await assertCurrent(); return projectInstagramProfile(payload, userId); } finally { await session.close(); }
}
