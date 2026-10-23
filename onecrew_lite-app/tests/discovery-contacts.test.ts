import assert from "node:assert/strict";
import { test } from "node:test";
import { discoveryContactPlatform, discoveryNonContact, discoverySocialProfile, extractDiscoveryContacts } from "../src/discovery/contacts.ts";

test("原公开联系方式提取保留邮箱大小写、Telegram明确标签及群组，不推断裸handle", () => {
  const rows = extractDiscoveryContacts("Business: Creator@EXAMPLE.COM Telegram: @creator_123 https://telegram.me/+InviteCode https://t.me/share/url @barehandle", "https://x.com/creator", "123", 10, "author_published");
  assert.deepEqual(rows.map(row => row.normalized_value), ["Creator@example.com", "https://t.me/+InviteCode", "https://t.me/creator_123"]);
  assert.equal(rows.every(row => row.native_author_id === "123" && row.attribution === "author_published"), true);
  assert.equal(extractDiscoveryContacts("https://site.example/a@b.com x..y@example.com shortened@example.com...", "https://x.com/creator", "123", 10, "author_published").length, 0);
});

test("原公众号和商务微信约定保留来源，地点、邮箱前缀和未确认页面不产生微信", () => {
  for (const text of ["公众号叫「外设研究」", "公众号是『外设研究』", "微信公众号名称：【外设研究】", "公众号：外设研究"]) {
    const rows = extractDiscoveryContacts(text, "https://space.bilibili.com/123", "123", 10, "author_published");
    assert.equal(rows.length, 1); assert.equal(rows[0]!.kind, "wechat_official"); assert.equal(rows[0]!.normalized_value, "外设研究");
    assert.deepEqual(extractDiscoveryContacts(text, "https://example.com", "123", 10, "unconfirmed"), []);
  }
  for (const [text, normalized] of [["商务合作V：chendahuang6（备注意向）", "chendahuang6"], ["商务合作 vx: creator_123", "creator_123"], ["🌍： Gk_lyy", "Gk_lyy"], ["🌍V510626 出海、AI账号", "V510626"]]) {
    assert.equal(extractDiscoveryContacts(text!, "https://x.com/creator", "123", 10, "author_published").find(row => row.kind === "wechat")?.normalized_value, normalized);
  }
  for (const text of ["🌍 London", "🌍: London", "🌍：Beijing", "V for victory", "商务合作V：email@example.com", "🌍V510626toolongforwechat000000"]) assert.equal(extractDiscoveryContacts(text, "https://x.com/creator", "123", 10, "author_published").some(row => row.kind === "wechat"), false);
});

test("原社交链接必须账号路径，内容和商品链接不能当联系目的地", () => {
  const author = "MS4wLjABAAAA_native_author_123456789";
  assert.equal(discoverySocialProfile(`https://www.douyin.com/user/${author}`), true); assert.equal(discoveryContactPlatform(`https://www.douyin.com/user/${author}`), "douyin");
  for (const value of ["https://www.douyin.com/video/7688639864627214835", "https://www.douyin.com/user/editable_douyin_number", `https://www.douyin.com/user/${author}?other=1`, "https://x.com/creator/status/123", "https://facebook.com/groups/123"]) assert.equal(discoverySocialProfile(value), false);
  assert.equal(discoveryNonContact("https://www.aritzia.com/us/en/contact-us"), true);
  assert.equal(discoveryNonContact("https://creator.example/contact"), false);
  const text = "Follow the guests:\nhttps://instagram.com/guest\n\nMy account https://instagram.com/creator";
  assert.deepEqual(extractDiscoveryContacts(text, "https://youtube.com/channel/UC1234567890123456789012", "author", 10, "author_published").map(row => row.normalized_value), ["https://instagram.com/creator"]);
});
