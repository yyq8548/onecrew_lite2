import assert from "node:assert/strict";
import { test } from "node:test";
import { blueskyProfilePlan, blueskyPostPlan, blueskyUriFromSource } from "../src/platforms/opencli/bluesky.ts";

const did = "did:plc:abcdefghijklmnopqrstuvwx";
test("Bluesky原生资料绑定DID，未知粉丝不推算", () => {
  const row = { did, handle: "author.bsky.social", display_name: "Author", biography: "Business: studio@example.com", follower_count: null, profile_url: `https://bsky.app/profile/${did}` };
  assert.equal(blueskyProfilePlan(did).decode([row]).follower_count, null);
  assert.throws(() => blueskyProfilePlan(did).decode([{ ...row, follower_count: undefined }]));
  assert.throws(() => blueskyProfilePlan("author.bsky.social"));
});
test("Bluesky帖子和评论验证原生URI与来源，公开邮箱链接仅为观察值", () => {
  const uri = `at://${did}/app.bsky.feed.post/first`; const source = `https://bsky.app/profile/${did}/post/first`;
  const row = { author_id: did, author_handle: "author.bsky.social", post_uri: uri, source_url: source, text: "Native original text", public_links: ["mailto:studio@example.org"], comments: [] };
  assert.equal(blueskyUriFromSource(source), uri);
  assert.equal(blueskyPostPlan(uri).decode([row]).text, row.text);
  const reply = { post_uri: `at://${did}/app.bsky.feed.post/reply`, source_url: `https://bsky.app/profile/${did}/post/reply`, text: "原始评论" };
  assert.equal(blueskyPostPlan(uri, true).decode([{ ...row, comments: [reply] }]).comments.length, 1);
  assert.throws(() => blueskyPostPlan(uri).decode([{ ...row, comments: [reply] }]));
  assert.throws(() => blueskyPostPlan(uri, true).decode([{ ...row, comments: [reply, reply] }]));
  assert.throws(() => blueskyPostPlan(uri).decode([{ ...row, public_links: ["mailto:studio@example.org?subject=hello"] }]));
});
