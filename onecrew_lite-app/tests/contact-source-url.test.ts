import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import { test } from "node:test";
import { equivalentSourceUrl, sourceUrlScript } from "../../onecrew_lite-opencli/onecrew-browser-search/source-url.mjs";
import { discoveryObservesLink } from "../src/discovery/contact-navigation.ts";

test("真实Instagram简介中的联系账号链接允许跟踪参数与尾斜线差异，保留不同账号", () => {
  const [profile] = JSON.parse(readFileSync(new URL("./fixtures/instagram-visible-profile/tssitlidze_sofia.json", import.meta.url), "utf8"));
  const contact = profile.biography.split("\n").find((line: string) => line.startsWith("www.instagram.com/"));
  const target = "https://www.instagram.com/pr.tsitlidze_sofia/";
  assert.equal(equivalentSourceUrl(contact, target), true);
  assert.equal(discoveryObservesLink(profile.biography, target), true);
  assert.equal(equivalentSourceUrl(contact, profile.profile_url), false);
  assert.equal(runInNewContext(sourceUrlScript(`equivalentSourceUrl(${JSON.stringify(contact)},${JSON.stringify(target)})`), { URL }), true);
});

test("LinkedIn同账号非本人参数与官网格式等价，另一账号和另一页面用途保持不同", () => {
  const source = "https://www.linkedin.com/in/aliciafoo98/";
  assert.equal(equivalentSourceUrl("https://linkedin.com/in/aliciafoo98?isSelfProfile=false&utm_source=profile", source), true);
  assert.equal(equivalentSourceUrl(`${source}?isSelfProfile=true`, source), false);
  assert.equal(equivalentSourceUrl(`${source}?redirect=/in/another/`, source), false);
  assert.equal(equivalentSourceUrl(`${source}?isSelfProfile=false&isSelfProfile=true`, source), false);
  assert.equal(equivalentSourceUrl("https://www.linkedin.com/in/another/", source), false);
  assert.equal(equivalentSourceUrl(`${source}overlay/contact-info/`, source), false);
  assert.equal(equivalentSourceUrl(`${source}overlay/contact-info?trk=profile`, `${source}overlay/contact-info/`), true);
});

test("所有平台共同入口只忽略格式与跟踪参数，保留账号、帖子和页面参数", () => {
  const cases: Array<[string, string]> = [
    ["https://twitter.com/person/?utm_source=profile", "https://x.com/person"],
    ["https://facebook.com/profile.php?fbclid=tracking&id=123&sk=directory_links", "https://www.facebook.com/profile.php?sk=directory_links&id=123"],
    ["https://pinterest.com/person?utm_source=profile", "https://www.pinterest.com/person/"],
    ["https://youtube.com/watch?utm_source=profile&v=abcdefghijk", "https://www.youtube.com/watch?v=abcdefghijk"],
    ["https://tiktok.com/@person?utm_source=profile", "https://www.tiktok.com/@person/"],
    ["https://reddit.com/user/person?utm_source=profile", "https://www.reddit.com/user/person/"],
    ["https://space.bilibili.com/123?utm_source=profile", "https://space.bilibili.com/123/"],
    ["https://douyin.com/user/person?utm_source=profile", "https://www.douyin.com/user/person/"],
    ["https://weibo.com/u/123?utm_source=profile", "https://www.weibo.com/u/123/"],
    ["https://zhihu.com/people/person?utm_source=profile", "https://www.zhihu.com/people/person/"],
    ["https://pixiv.net/users/123?utm_source=profile", "https://www.pixiv.net/users/123/"],
    ["https://bsky.app/profile/person?utm_source=profile", "https://bsky.app/profile/person/"],
    ["https://news.ycombinator.com/user?id=person&utm_source=profile", "https://news.ycombinator.com/user?id=person"],
  ];
  for (const [actual, expected] of cases) assert.equal(equivalentSourceUrl(actual, expected), true, expected);
  for (const [actual, expected] of [
    ["https://facebook.com/profile.php?id=124&sk=directory_links", "https://www.facebook.com/profile.php?id=123&sk=directory_links"],
    ["https://facebook.com/profile.php?id=123&sk=about", "https://www.facebook.com/profile.php?id=123&sk=directory_links"],
    ["https://youtube.com/watch?v=abcdefghijk&list=other", "https://www.youtube.com/watch?v=abcdefghijk"],
    ["https://pinterest.com/pin/124", "https://www.pinterest.com/pin/123"],
    ["https://news.ycombinator.com/user?id=another", "https://news.ycombinator.com/user?id=person"],
    ["https://example.com/#account/2", "https://example.com/#account/1"],
    ["https://linkedin.com.evil.test/in/person/", "https://www.linkedin.com/in/person/"],
    ["http://www.linkedin.com/in/person/", "https://www.linkedin.com/in/person/"],
    ["https://viewer@www.linkedin.com/in/person/", "https://www.linkedin.com/in/person/"],
  ] as const) assert.equal(equivalentSourceUrl(actual, expected), false, actual);
});
