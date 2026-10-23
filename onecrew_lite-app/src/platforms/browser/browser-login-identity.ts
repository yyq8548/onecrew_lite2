import { randomBytes, randomUUID } from "node:crypto";
import { z } from "zod";
import { LosslessNumber } from "lossless-json";
import { parseSetCookie } from "set-cookie-parser";
import { createReadSession } from "../../native-capabilities/network.ts";
import type { BrowserCookie } from "../../native-capabilities/chrome-cookies.ts";
import { parseStoredJson } from "../../contracts/stored-json.ts";
import { parseApplicationJson, stringifyApplicationJson } from "../../contracts/application-json.ts";
import { instagramReadHeaders, nativeProxy, xReadHeaders } from "../candidates/social-read.ts";
import { readNativeXGraphql } from "./native-x-search.ts";
import type { PlatformProfileSecret } from "./platform-vault.ts";
import { CreatorProviderReadError } from "../candidates/projection.ts";

type Geo = { timezone: string | null; accept_language: string | null };
export class InstagramLoginSessionError extends Error { constructor() { super("instagram.local_session_invalid"); } }
const instagramStoredSession = z.object({
  uuids: z.object({ phone_id: z.string(), uuid: z.string(), client_session_id: z.string(), advertising_id: z.string(), android_device_id: z.string(), request_id: z.string(), tray_session_id: z.string() }).passthrough(),
  device_settings: z.object({ app_version: z.string(), android_version: z.number().int().min(0).max(4294967295), android_release: z.string(), dpi: z.string(), resolution: z.string(), manufacturer: z.string(), device: z.string(), model: z.string(), cpu: z.string(), version_code: z.string() }).passthrough(),
  user_agent: z.string(), cookies: z.record(z.string(), z.string()).default({}), authorization_data: z.object({ ds_user_id: z.string(), sessionid: z.string(), should_use_header_over_cookies: z.boolean() }).passthrough(),
  mid: z.string().nullable().default(null), ig_www_claim: z.string().nullable().default(null), ig_u_rur: z.string().nullable().default(null), country: z.string().default("US"), locale: z.string().default("en_US"), timezone_offset: z.union([z.number().int().safe(), z.bigint().min(-9223372036854775808n).max(9223372036854775807n)]).default(0),
}).passthrough();
export function restoreInstagramBrowserSession(raw: string) { try { return instagramStoredSession.parse(parseApplicationJson(raw)); } catch { throw new InstagramLoginSessionError(); } }
export class InstagramLoginResponseError extends Error {
  readonly status: number; readonly code: string | null;
  constructor(status: number, code: unknown = null) { super("instagram.account_api_response_rejected"); this.status = status; this.code = typeof code === "string" && /^[A-Za-z0-9_]{1,80}$/u.test(code) ? code : null; }
}
export function instagramLoginResponseError(status: number, body: string): InstagramLoginResponseError {
  const parsed = (() => { try { return z.record(z.string(), z.unknown()).parse(parseStoredJson(body)); } catch { return null; } })();
  return new InstagramLoginResponseError(status, parsed?.error_type);
}
function timezoneOffset(timeZone: string | null): number {
  if (!timeZone) return 0;
  const now = Date.now(), parts = new Intl.DateTimeFormat("en-US", { timeZone, hourCycle: "h23", year: "numeric", month: "numeric", day: "numeric", hour: "numeric", minute: "numeric", second: "numeric" }).formatToParts(now);
  const value = (type: Intl.DateTimeFormatPartTypes) => Number(parts.find(part => part.type === type)!.value);
  return Math.round((Date.UTC(value("year"), value("month") - 1, value("day"), value("hour"), value("minute"), value("second")) - Math.floor(now / 1000) * 1000) / 1000);
}
export function createInstagramBrowserSession(cookies: readonly BrowserCookie[], geo: Geo) {
  const jar = Object.fromEntries(cookies.map(cookie => [cookie.name, cookie.value])), sessionid = jar.sessionid?.trim();
  if (!sessionid || sessionid.length < 30) throw new InstagramLoginSessionError();
  const id = jar.ds_user_id?.trim() || /^\d+/u.exec(sessionid)?.[0]; if (!id) throw new InstagramLoginSessionError();
  const tag = geo.accept_language?.split(",")[0]?.trim() || "en-US", locale = tag.replaceAll("-", "_"), country = tag.split("-")[1]?.toUpperCase() || "US";
  const device_settings = { app_version: "428.0.0.47.67", android_version: 34, android_release: "14", dpi: "480dpi", resolution: "1344x2992", manufacturer: "Google/google", device: "husky", model: "Pixel 8 Pro", cpu: "husky", version_code: "961145276" };
  const uuids = { phone_id: randomUUID(), uuid: randomUUID(), client_session_id: randomUUID(), advertising_id: randomUUID(), android_device_id: `android-${randomBytes(8).toString("hex")}`, request_id: randomUUID(), tray_session_id: randomUUID() };
  return { uuids, device_settings, user_agent: `Instagram 428.0.0.47.67 Android (34/14; 480dpi; 1344x2992; Google/google; Pixel 8 Pro; husky; husky; ${locale}; 961145276)`, cookies: jar, authorization_data: { ds_user_id: id, sessionid, should_use_header_over_cookies: true }, mid: jar.mid?.trim() || null, ig_www_claim: null as string | null, ig_u_rur: null as string | null, country, locale, timezone_offset: timezoneOffset(geo.timezone) };
}
export function refreshInstagramBrowserSession(settings: Pick<z.infer<typeof instagramStoredSession>, "mid" | "ig_www_claim" | "ig_u_rur" | "cookies">, headers: Readonly<Pick<Headers, "get" | "getSetCookie">>): void {
  settings.mid = headers.get("ig-set-x-mid") || settings.mid; settings.ig_www_claim = headers.get("x-ig-set-www-claim") || settings.ig_www_claim; settings.ig_u_rur = headers.get("ig-set-ig-u-rur") || settings.ig_u_rur;
  for (const cookie of parseSetCookie(headers.getSetCookie(), { decodeValues: false, split: false })) if (cookie.name && cookie.value && cookie.value !== '""') settings.cookies[cookie.name] = cookie.value;
}
function providerId(value: unknown): string { const id = typeof value === "string" ? value : value instanceof LosslessNumber ? value.value : typeof value === "number" && Number.isSafeInteger(value) ? String(value) : ""; if (!id.trim() || !id.isWellFormed() || /\p{Cc}/u.test(id)) throw new Error("browser.provider_identity_invalid"); return id; }
export async function verifyBrowserLoginIdentity(platform: "x" | "instagram", base: PlatformProfileSecret, geo: Geo, signal: AbortSignal, assertCurrent: () => Promise<void>, reuseSession = false): Promise<{ providerAccountId: string; username: string; secret: PlatformProfileSecret }> {
  if (platform === "instagram") {
    let stored: z.infer<typeof instagramStoredSession> | null = null;
    if (reuseSession) {
      if (!base.instagram_session_settings?.trim()) throw new InstagramLoginSessionError();
      stored = restoreInstagramBrowserSession(base.instagram_session_settings);
    }
    const settings = stored ?? createInstagramBrowserSession(base.cookies, geo), secret = { ...base, provider_account_id: settings.authorization_data.ds_user_id, instagram_session_settings: stringifyApplicationJson(settings) }, path = "/api/v1/accounts/current_user/";
    let headers: Readonly<Record<string, string>>; try { headers = instagramReadHeaders(secret); } catch (error) { if (error instanceof z.ZodError || error instanceof CreatorProviderReadError) throw new InstagramLoginSessionError(); throw error; }
    const session = await createReadSession(["https://i.instagram.com"], nativeProxy(base.proxy), { browser: "okhttp_5", headers, allowPath: url => url.pathname === path, classifyHttpFailure: (status, body) => instagramLoginResponseError(status, body), observeResponseHeaders: responseHeaders => refreshInstagramBrowserSession(settings, responseHeaders) });
    try { await assertCurrent(); const text = await session.getText(new URL(`https://i.instagram.com${path}?edit=true`), signal), raw = z.record(z.string(), z.unknown()).parse(parseStoredJson(text)); if (raw.status !== "ok") throw instagramLoginResponseError(200, text); const payload = z.object({ user: z.object({ pk: z.unknown(), username: z.string().min(1) }) }).parse(raw); await assertCurrent(); const id = providerId(payload.user.pk); if (id !== settings.authorization_data.ds_user_id) throw new Error("browser.instagram_identity_mismatch"); return { providerAccountId: id, username: payload.user.username, secret: { ...secret, instagram_session_settings: stringifyApplicationJson(settings) } }; } finally { await session.close(); }
  }
  const path = "/i/api/1.1/account/multi/list.json", session = await createReadSession(["https://x.com"], nativeProxy(base.proxy), { browser: "chrome_149", headers: xReadHeaders(base), allowPath: url => url.pathname === path });
  let username: string;
  try { await assertCurrent(); const payload = await session.getJson(new URL(`https://x.com${path}`), signal), list = z.object({ users: z.array(z.object({ screen_name: z.string().min(1) })).min(1) }).safeParse(payload); username = list.success ? list.data.users[0]!.screen_name : z.array(z.object({ user: z.object({ screen_name: z.string().min(1) }) })).min(1).parse(payload)[0]!.user.screen_name; } finally { await session.close(); }
  await assertCurrent(); const payload = z.object({ data: z.object({ user: z.object({ result: z.object({ rest_id: z.string(), core: z.object({ screen_name: z.string() }).optional(), legacy: z.object({ screen_name: z.string() }).optional() }) }) }) }).parse(await readNativeXGraphql(base, "UserByScreenName", { screen_name: username, withSafetyModeUserFields: true }, "GET", signal, assertCurrent));
  const result = payload.data.user.result, id = providerId(result.rest_id), actualName = result.core?.screen_name || result.legacy?.screen_name; if (!actualName || actualName.toLowerCase() !== username.toLowerCase()) throw new Error("browser.x_identity_mismatch"); await assertCurrent(); return { providerAccountId: id, username: actualName, secret: { ...base, provider_account_id: id } };
}
