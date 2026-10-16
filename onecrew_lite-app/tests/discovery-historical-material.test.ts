import assert from "node:assert/strict";
import { test } from "node:test";
import { validateHistoricalDiscoveryContent, validateHistoricalDiscoveryProfile } from "../src/discovery/historical-native-material.ts";

test("历史知乎原生合同保存未知粉丝，内容必须绑定原作者与文章或回答定位符", () => {
  const author = { author_id: "a".repeat(32), url_token: "creator-123", author_label: "Creator", author_url: "https://www.zhihu.com/people/creator-123" }, profile = { ...author, follower_count: null, bio: "商务联系 contact@example.com" };
  assert.deepEqual(validateHistoricalDiscoveryProfile("zhihu", author.author_id, author.author_url, profile), profile);
  assert.throws(() => validateHistoricalDiscoveryProfile("zhihu", author.author_id, author.author_url, { ...profile, follower_count: -1 }));
  const article = { ...author, article_id: "123", source_url: "https://zhuanlan.zhihu.com/p/123", title: "原始文章", text: "原始正文", public_links: ["mailto:contact@example.com"] };
  assert.deepEqual(validateHistoricalDiscoveryContent("zhihu", author.author_id, article.source_url, article), article);
  assert.throws(() => validateHistoricalDiscoveryContent("zhihu", "b".repeat(32), article.source_url, article), /binding/);
  assert.throws(() => validateHistoricalDiscoveryContent("zhihu", author.author_id, article.source_url, { ...article, public_links: ["https://example.com/?token=private"] }));
});

test("历史Hacker News与Pixiv合同保持无粉丝事实，拒绝另一作者和评论注入", () => {
  const hn = { author_id: "creator", username: "creator", biography_html: "<p>Biography</p>", biography: "Biography", profile_url: "https://news.ycombinator.com/user?id=creator", created: 1, karma: 20, follower_count: null };
  assert.deepEqual(validateHistoricalDiscoveryProfile("hackernews", "creator", hn.profile_url, hn), hn);
  const item = { item_id: 123, author_id: "creator", source_url: "https://news.ycombinator.com/item?id=123", title: "Native post", text_html: "<p>Native text</p>", text: "Native text", external_url: null, comment_ids: [124], comments: [] };
  assert.deepEqual(validateHistoricalDiscoveryContent("hackernews", "creator", item.source_url, item), item);
  assert.throws(() => validateHistoricalDiscoveryContent("hackernews", "creator", item.source_url, { ...item, comment_ids: [123] }));
  assert.throws(() => validateHistoricalDiscoveryContent("hackernews", "creator", item.source_url, { ...item, comments: [{ text: "comment" }] }));
  const pixiv = { adapter_version: "1.0.0", author_id: "123", author_label: "Creator", profile_url: "https://www.pixiv.net/users/123", follower_count: null, bio: "原生简介", public_links: [], evidence_source: "native_ajax_response" };
  assert.deepEqual(validateHistoricalDiscoveryProfile("pixiv", "123", pixiv.profile_url, pixiv), pixiv);
  const artwork = { post_id: "456", author_id: "123", source_url: "https://www.pixiv.net/artworks/456", text: "原生作品说明", text_truncated: false };
  assert.deepEqual(validateHistoricalDiscoveryContent("pixiv", "123", artwork.source_url, artwork), artwork);
  assert.throws(() => validateHistoricalDiscoveryContent("pixiv", "124", artwork.source_url, artwork));
});
