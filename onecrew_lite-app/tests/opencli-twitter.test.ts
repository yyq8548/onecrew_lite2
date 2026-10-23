import assert from "node:assert/strict";
import { test } from "node:test";
import { twitterPostIdentityPlan, twitterPostContentPlan, twitterLanguagePlan, twitterRecentContentPlan } from "../src/platforms/opencli/twitter-posts.ts";

const identity = { adapter_version: "1.0.0", post_id: "1234567890123456789", account_id: "987654321012345678", handle: "creator_1", profile_url: "https://x.com/creator_1", source_url: "https://x.com/creator_1/status/1234567890123456789", identity_kind: "native_post_author_id", evidence_source: "x_tweet_detail" };
test("X原帖子合同严格绑定原生作者与帖子，不把可见链接当内容证明", () => {
  assert.equal(twitterPostIdentityPlan(identity.handle, identity.post_id).decode([identity]).account_id, identity.account_id);
  assert.throws(() => twitterPostIdentityPlan(identity.handle, identity.post_id).decode([{ ...identity, unknown: true }]));
  assert.throws(() => twitterPostIdentityPlan("Creator_1", identity.post_id));
  const plan = twitterPostContentPlan(identity.handle, identity.post_id, identity.account_id);
  const content = { adapter_version: "1.0.0", identity, text: "Original public post", text_truncated: false, text_source: "legacy_full_text", created_at: "Fri Sep 25 23:00:10 +0000 2026", has_media: true, quotes_another_post: false, expanded_urls: ["https://example.com/tool"] };
  assert.equal(plan.decode([content]).text, content.text);
  for (const change of [{ text: "  " }, { text: "中".repeat(2667) }, { expanded_urls: ["javascript:alert(1)"] }, { expanded_urls: ["https://example.com", "https://example.com"] }, { identity: { ...identity, account_id: "42" } }]) assert.throws(() => plan.decode([{ ...content, ...change }]));
  const language = twitterLanguagePlan(identity.handle, identity.post_id, identity.account_id);
  assert.equal(language.decode([{ author_id: identity.account_id, source_url: identity.source_url, comments: [] }]).author_id, identity.account_id);
  assert.throws(() => language.decode([{ author_id: identity.handle, source_url: identity.source_url }]));
});
test("X近期内容排除转发和其他作者，Unicode按原字节上限截取", () => {
  const plan = twitterRecentContentPlan("creator_1");
  const row = { id: "123456789", author: "Creator_1", url: "https://x.com/Creator_1/status/123456789", is_retweet: false, has_media: true, created_at: "Fri Sep 25 23:00:10 +0000 2026", text: "中".repeat(1000) };
  const retweet = { ...row, id: "123456790", url: "https://x.com/Creator_1/status/123456790", is_retweet: true };
  const foreign = { ...row, id: "987654321", author: "Other", url: "https://x.com/Other/status/987654321" };
  const result = plan.decode([row, retweet, foreign]);
  assert.equal(result.sampled_posts, 2); assert.equal(result.excluded_retweets, 1);
  assert.equal(result.original_posts.length, 1); assert.equal(result.original_posts[0]!.text, "中".repeat(666));
  assert.equal(result.original_posts[0]!.text_truncated, true); assert.equal(result.identity_kind, "mutable_handle");
  assert.throws(() => plan.decode([row, row]));
  assert.throws(() => plan.decode([row, { ...foreign, url: row.url }]));
});
