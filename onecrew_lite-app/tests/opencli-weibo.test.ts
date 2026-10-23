import assert from "node:assert/strict";
import { test } from "node:test";
import { weiboProfilePlan, weiboPostPlan, weiboCommentsPlan } from "../src/platforms/opencli/weibo.ts";

test("微博原生资料保留未知与零粉丝，严格拒绝重定向和未审核字段", () => {
  const plan = weiboProfilePlan("12345");
  const row = { id: "12345", name: "Author", follower_count: null, profile_url: "https://weibo.com/u/12345", bio: "creator@example.com", self_declared_location: "" };
  assert.equal(plan.decode([row]).follower_count, null);
  assert.equal(plan.decode([{ ...row, follower_count: 0 }]).follower_count, 0);
  assert.throws(() => plan.decode([{ ...row, cookie: "private" }]));
  assert.throws(() => plan.decode([{ ...row, profile_url: row.profile_url + "?redirect=1" }]));
  assert.throws(() => plan.decode([{ ...row, follower_count: undefined }]));
});
test("微博帖子保留作者原文、转发与完整性事实，不合并转发正文", () => {
  const source = "https://weibo.com/7407613416/RkNMBxdqR";
  const fields = { id: "5091234567891234", mblogid: "RkNMBxdqR", url: source, author: "作者", text: "中".repeat(4001), retweeted_text: "其他作者正文" };
  const rows = Object.entries(fields).map(([field, value]) => ({ field, value }));
  const plan = weiboPostPlan(source); const result = plan.decode(rows);
  assert.equal(result.text, "中".repeat(4000)); assert.equal(result.text_truncated, true);
  assert.equal(result.is_repost, true); assert.equal(result.text_completeness_known, false);
  assert.throws(() => plan.decode([...rows, rows[0]]));
  assert.throws(() => plan.decode(rows.map(row => row.field === "url" ? { ...row, value: source + "?x=1" } : row)));
  assert.throws(() => weiboPostPlan("https://weibo.com/9007199254740992/abc"));
});
test("微博评论只保留前五条原顺序的原文，拒绝排名漂移及截断", () => {
  const rows = [1, 2, 3, 4, 5].map(rank => ({ rank, id: `1234${rank}`, text: `原评论${rank}`, text_truncated: false, author: "not author evidence" }));
  const plan = weiboCommentsPlan("5091234567891234");
  assert.deepEqual(plan.decode(rows)[0], { id: "12341", text: "原评论1" });
  assert.throws(() => plan.decode([...rows, { ...rows[0], rank: 6 }]));
  assert.throws(() => plan.decode(rows.map((row, index) => index === 1 ? { ...row, rank: 6 } : row)));
  assert.throws(() => plan.decode(rows.map(row => ({ ...row, text_truncated: true }))));
});
