import assert from "node:assert/strict";
import { test } from "node:test";
import { decodeBlueskyProfile, decodeBlueskyPost, blueskyUriFromSource } from "../src/platforms/opencli/bluesky.ts";

const did = "did:plc:abcdefghijklmnopqrstuvwx";
test("Bluesky原生资料绑定DID，未知粉丝不推算", () => {
  const row = { did, handle: "author.bsky.social", display_name: "Author", biography: "Business: studio@example.com", follower_count: null, profile_url: `https://bsky.app/profile/${did}` };
  assert.equal(decodeBlueskyProfile(did, [row]).follower_count, null);
  assert.throws(() => decodeBlueskyProfile(did, [{ ...row, follower_count: undefined }]));
  assert.throws(() => decodeBlueskyProfile("author.bsky.social", [row]));
});
test("Bluesky帖子和评论验证原生URI与来源，公开邮箱链接仅为观察值", () => {
  const uri = `at://${did}/app.bsky.feed.post/first`; const source = `https://bsky.app/profile/${did}/post/first`;
  const row = { author_id: did, author_handle: "author.bsky.social", post_uri: uri, source_url: source, text: "Native original text", public_links: ["mailto:studio@example.org"], comments: [] };
  assert.equal(blueskyUriFromSource(source), uri);
  assert.equal(decodeBlueskyPost(uri, [row]).text, row.text);
  const reply = { post_uri: `at://${did}/app.bsky.feed.post/reply`, source_url: `https://bsky.app/profile/${did}/post/reply`, text: "原始评论" };
  assert.equal(decodeBlueskyPost(uri, [{ ...row, comments: [reply] }], true).comments.length, 1);
  assert.throws(() => decodeBlueskyPost(uri, [{ ...row, comments: [reply] }]));
  assert.throws(() => decodeBlueskyPost(uri, [{ ...row, comments: [reply, reply] }], true));
  assert.throws(() => decodeBlueskyPost(uri, [{ ...row, public_links: ["mailto:studio@example.org?subject=hello"] }]));
});
