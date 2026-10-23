import assert from "node:assert/strict";
import { test } from "node:test";
import { projectXProfile, projectInstagramProfile } from "../src/platforms/candidates/projection.ts";
import { xProfileReadUrl, instagramReadHeaders, nativeProxy } from "../src/platforms/candidates/social-read.ts";
import type { PlatformProfileSecret } from "../src/platforms/browser/platform-vault.ts";

test("X真实合同投影核对选定原生ID、资料来源与u64粉丝，拒绝未知计数", () => {
  const row = { rest_id: "123", core: { name: "Creator", screen_name: "creator" }, legacy: { description: "Business: creator@example.org", location: "US", followers_count: 9007199254740993n, entities: { url: { urls: [{ expanded_url: "https://creator.example", display_url: "creator.example" }] } } } };
  const result = projectXProfile({ data: { user: { result: row } } }, "123");
  assert.equal(result.account.external_id, "123"); assert.equal(result.profile.follower_count, 9007199254740993n);
  assert.equal(result.evidence.some(value => value.kind === "profile_field" && value.source === "profile_location"), true);
  assert.throws(() => projectXProfile(row, "999"), { kind: "identity_mismatch" });
  assert.throws(() => projectXProfile({ ...row, legacy: { ...row.legacy, followers_count: undefined } }, "123"));
  const url = xProfileReadUrl("creator"); assert.equal(url.origin, "https://x.com");
  assert.equal(url.pathname.endsWith("/UserByScreenName"), true);
  assert.throws(() => xProfileReadUrl("../settings"));
});
test("Instagram真实合同投影保留公开邮件及资料链接，原会话负时区与OkHttp请求头不改写", () => {
  const result = projectInstagramProfile({ user: { pk: 123n, username: "creator", full_name: "Creator", biography: "Business", bio_links: [{ url: "https://creator.example" }], external_url: "https://creator.example", public_email: "creator@example.org", follower_count: 0 } }, "123");
  assert.equal(result.profile.follower_count, 0); assert.equal(result.profile.website_urls.length, 1);
  assert.equal(result.evidence.some(value => value.kind === "profile_field" && value.text === "creator@example.org"), true);
  assert.throws(() => projectInstagramProfile({ user: { pk: "999" } }, "123"), { kind: "identity_mismatch" });
  const session = { uuids: { uuid: "device", phone_id: "family", android_device_id: "android-device" }, user_agent: "Original Android app user agent", cookies: {}, authorization_data: { ds_user_id: "123", sessionid: "existing-session", should_use_header_over_cookies: true }, locale: "en_US", country: "US", timezone_offset: -28800 };
  const secret: PlatformProfileSecret = { provider_account_id: "123", cookies: [], x_chat_passcode: null, instagram_session_settings: JSON.stringify(session), telegram_api_id: null, telegram_api_hash: null, telegram_session: null, profile_name: "", browser_profile_dir: "", fingerprint_seed: "" };
  const headers = instagramReadHeaders(secret);
  assert.equal(headers["x-ig-timezone-offset"], "-28800"); assert.equal(headers["user-agent"], session.user_agent);
  assert.equal(headers.authorization, `Bearer IGT:2:${Buffer.from(JSON.stringify(session.authorization_data)).toString("base64")}`);
  assert.equal(nativeProxy({ kind: "socks5", host: "127.0.0.1", port: 1080, username: "u@x", password: "p:a" }), "socks5://u%40x:p%3Aa@127.0.0.1:1080");
});
