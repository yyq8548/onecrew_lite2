import assert from "node:assert/strict";
import { test } from "node:test";
import { redditProfilePlan, redditRecentPlan, redditLanguagePlan, redditPostPlan } from "../src/platforms/opencli/reddit.ts";

test("Reddit原生资料保留账户大小写、原生ID和未知粉丝，近期定位去重", () => {
  const row = { adapter_version: "1.0.0", id: "t2_user", username: "Example", biography: "Public biography", follower_count: null, profile_url: "https://www.reddit.com/user/Example/", evidence_source: "native_profile_response" };
  const plan = redditProfilePlan("example", "t2_user");
  assert.equal(plan.decode([row]).username, "Example");
  assert.throws(() => plan.decode([{ ...row, id: "t2_other" }]));
  const recent = redditRecentPlan("Example", "t2_user");
  const locator = { source_url: "https://www.reddit.com/comments/abc/" };
  assert.deepEqual(recent.decode([locator]), [locator.source_url]);
  assert.throws(() => recent.decode([locator, locator]));
  assert.throws(() => recent.decode([{ source_url: locator.source_url + "?x=1" }]));
});
test("Reddit原帖不合并评论正文，语言样本只保留同帖评论来源", () => {
  const source = "https://www.reddit.com/comments/abc/";
  const post = { type: "POST", id: "t3_abc", native_author_id: "t2_user", author: "Example", source_url: source, text: "日".repeat(3000), text_truncated: false };
  const reply = { type: "L0", id: "t1_reply", source_url: source + "_/reply/", text: "日".repeat(1000), score: 20 };
  const original = redditPostPlan("t2_user", source).decode([post, reply]);
  assert.equal(original.text, "日".repeat(2666)); assert.equal(original.text_truncated, true);
  assert.equal("comments" in original, false);
  const sample = redditLanguagePlan("example", "t2_user", source).decode([post, reply]);
  assert.equal(sample.comments[0]!.text, "日".repeat(666));
  assert.deepEqual(Object.keys(sample.comments[0]!), ["source_url", "text"]);
  assert.throws(() => redditLanguagePlan("other", "t2_user", source).decode([post, reply]));
  assert.throws(() => redditLanguagePlan("example", "t2_user", source).decode([post, { ...reply, source_url: "https://www.reddit.com/comments/other/_/reply/" }]));
});
