import assert from "node:assert/strict";
import { test } from "node:test";
import { douyinProfilePlan, douyinContentPlan, douyinCommentsPlan } from "../src/platforms/opencli/douyin.ts";

const author = "MS4wLjABAAAAg5NIc2YQ9j1jGu9OUd4tJQc_C9cDFfpTWiLKTwWztLg";
const video = "7688639864627214835";
test("抖音资料只接受sec_uid，粉丝显示不推算精确数量", () => {
  const row = { adapter_version: "1.0.0", author_id: author, author_label: "Creator", profile_url: `https://www.douyin.com/user/${author}`, follower_count: null, follower_text: "1.2万", bio: "Business: creator@example.org", bio_truncated: false, public_links: [], evidence_source: "visible_profile_header" };
  const plan = douyinProfilePlan(author);
  assert.equal(plan.decode([row]).follower_count, null);
  assert.equal(plan.decode([{ ...row, follower_text: "12,345", follower_count: 12345 }]).follower_count, 12345);
  assert.throws(() => plan.decode([{ ...row, follower_count: 12000 }]));
  assert.throws(() => douyinProfilePlan("23667677481"));
});
test("抖音caption和评论绑定同一原生视频与作者，最多五条评论", () => {
  const base = { post_id: video, author_id: author, source_url: `https://www.douyin.com/video/${video}`, text: "Original caption", text_truncated: false };
  const caption = { ...base, author_label: "Creator", profile_url: `https://www.douyin.com/user/${author}`, evidence_source: "visible_native_video_caption" };
  assert.equal(douyinContentPlan(video).decode([caption]).author_id, author);
  const comments = { ...base, comments: [{ id: "101", parent_post_id: video, text: "" }], evidence_source: "visible_same_video_comments" };
  const plan = douyinCommentsPlan(video, author);
  assert.equal(plan.decode([comments]).comments.length, 1);
  assert.throws(() => plan.decode([{ ...comments, comments: [...comments.comments, ...comments.comments] }]));
  assert.throws(() => plan.decode([{ ...comments, comments: [{ ...comments.comments[0], parent_post_id: "99" }] }]));
});
