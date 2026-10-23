import test from "node:test";
import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import { createInstagramBrowserSession, refreshInstagramBrowserSession, restoreInstagramBrowserSession, InstagramLoginResponseError, InstagramLoginSessionError, instagramLoginResponseError, verifyBrowserLoginIdentity } from "../src/platforms/browser/browser-login-identity.ts";
import { instagramReadHeaders } from "../src/platforms/candidates/social-read.ts";
import { stringifyApplicationJson } from "../src/contracts/application-json.ts";
import { openCliLoginPage } from "../src/platforms/opencli/login-pages.ts";

test("Instagram original device, full routing cookies, serde defaults and signed i64 timezone survive local round trips", () => {
  const provider = "18446744073709551615", token = `${provider}:${randomBytes(30).toString("hex")}`, cookie = (name: string, value: string) => ({ name, value, domain: ".instagram.com", path: "/", secure: true, http_only: true });
  const session = createInstagramBrowserSession([cookie("sessionid", token), cookie("ds_user_id", provider), cookie("rur", "original-route"), cookie("csrftoken", "original-csrf")], { timezone: "America/New_York", accept_language: "en-US,en;q=0.9" });
  assert.equal(session.authorization_data.ds_user_id, provider); assert.equal(session.cookies.rur, "original-route"); assert.ok([-14400, -18000].includes(session.timezone_offset));
  const { cookies: _, mid: _mid, ig_www_claim: _claim, ig_u_rur: _rur, country: _country, locale: _locale, timezone_offset: _offset, ...required } = session;
  const restored = restoreInstagramBrowserSession(stringifyApplicationJson(required)); assert.deepEqual(restored.uuids, session.uuids); assert.deepEqual(restored.device_settings, session.device_settings); assert.deepEqual(restored.cookies, {}); assert.equal(restored.timezone_offset, 0); assert.equal(restored.country, "US"); assert.equal(restored.mid, null);
  const headers = new Headers({ "ig-set-x-mid": "new-mid", "x-ig-set-www-claim": "new-claim", "ig-set-ig-u-rur": "new-rur" }); headers.append("set-cookie", "rur=encoded%3Aroute; Path=/; HttpOnly"); headers.append("set-cookie", "csrftoken=new-csrf; Path=/"); headers.append("set-cookie", 'empty=""; Max-Age=0');
  refreshInstagramBrowserSession(session, headers); assert.equal(session.cookies.rur, "encoded%3Aroute"); assert.equal(session.cookies.csrftoken, "new-csrf"); assert.equal(session.cookies.empty, undefined); assert.equal(session.ig_u_rur, "new-rur");
  refreshInstagramBrowserSession(session, new Headers()); assert.equal(session.ig_u_rur, "new-rur");
  const signed = { ...session, timezone_offset: -9223372036854775808n }, secret = { provider_account_id: provider, cookies: [], x_chat_passcode: null, instagram_session_settings: stringifyApplicationJson(signed), telegram_api_id: null, telegram_api_hash: null, telegram_session: null, profile_name: "", browser_profile_dir: "", fingerprint_seed: "" };
  assert.equal(instagramReadHeaders(secret)["x-ig-timezone-offset"], "-9223372036854775808"); assert.equal(restoreInstagramBrowserSession(secret.instagram_session_settings).timezone_offset, -9223372036854775808n);
  assert.throws(() => restoreInstagramBrowserSession('{"uuids":{},"uuids":{}}'), InstagramLoginSessionError);
});

test("local session rejection and machine diagnostics never imply remote login or expose provider bodies", async () => {
  const base = { provider_account_id: "1", cookies: [], x_chat_passcode: null, instagram_session_settings: null, telegram_api_id: null, telegram_api_hash: null, telegram_session: null, profile_name: "", browser_profile_dir: "", fingerprint_seed: "" };
  await assert.rejects(verifyBrowserLoginIdentity("instagram", base, { timezone: null, accept_language: null }, new AbortController().signal, async () => {}, true), InstagramLoginSessionError);
  const error = instagramLoginResponseError(403, JSON.stringify({ error_type: "challenge_required", message: "Bearer SENTINEL", challenge: { url: "https://secret.invalid/SENTINEL" } })); assert.ok(error instanceof InstagramLoginResponseError); assert.equal(error.code, "challenge_required"); assert.equal(error.status, 403); assert.ok(!JSON.stringify(error).includes("SENTINEL")); assert.ok(!error.message.includes("SENTINEL"));
  assert.equal(instagramLoginResponseError(429, '{"error_type":"https://secret.invalid/SENTINEL"}').code, null);
  assert.equal(openCliLoginPage("twitter"), "https://x.com/"); assert.throws(() => openCliLoginPage("https://evil.invalid/")); assert.throws(() => openCliLoginPage("--profile other"));
});
