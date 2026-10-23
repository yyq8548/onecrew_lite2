import assert from "node:assert/strict";
import { test } from "node:test";
import { tiktokLanguagePlan, tiktokRecentLanguagePlan, bilibiliLanguagePlan, instagramRecentOriginalsPlan, instagramCommentLanguagePlan } from "../src/platforms/opencli/language.ts";
import { decodePublicWebRead } from "../src/platforms/opencli/web.ts";
import { creatorProfileSource } from "../src/platforms/opencli/profile-source.ts";

test("原TikTok语言合同按字节限制文本、去重评论且拒绝作者漂移", () => {
  const source = "https://www.tiktok.com/@creator/video/123";
  const post = { type: "POST", source_url: source, native_author_id: "456", author: "creator", text: "中".repeat(3000) };
  const reply = { type: "COMMENT", source_url: source, text: "日".repeat(1000) };
  const plan = tiktokLanguagePlan("creator", "456", source);
  const result = plan.decode([post, reply, reply]);
  assert.equal(result.text, "中".repeat(2666)); assert.equal(result.comments.length, 1);
  assert.equal(result.comments[0]!.text, "日".repeat(666));
  assert.throws(() => plan.decode([{ ...post, native_author_id: "789" }]));
  assert.deepEqual(tiktokRecentLanguagePlan("creator", "456").decode([{ source_url: source }]), [source]);
  assert.throws(() => tiktokRecentLanguagePlan("creator", "456").decode([{ source_url: source }, { source_url: source }]));
});
test("Bilibili与Instagram语言结果保留原生归属，评论不提升为作者正文", () => {
  const source = "https://www.bilibili.com/video/BV1xx411c7mD";
  assert.deepEqual(bilibiliLanguagePlan("123", null).decode([{ source_url: source }]), [source]);
  assert.throws(() => bilibiliLanguagePlan("123", source).decode([{ author_id: "999", source_url: source }]));
  const igSource = "https://www.instagram.com/reel/Abc123/";
  const recent = instagramRecentOriginalsPlan("creator", "123");
  const row = { post_id: "456", author_id: "123", source_url: igSource, text: "Original caption", text_truncated: false };
  assert.equal(recent.decode([row])![0]!.author_id, "123");
  assert.throws(() => recent.decode([row, row]));
  const post = { type: "POST", source_url: igSource, id: "Abc123", author: "creator", author_url: "https://www.instagram.com/creator/", identity_source: "visible_profile_link" };
  const reply = { type: "COMMENT", id: "789", source_url: "https://www.instagram.com/p/Abc123/c/789/", text: "Original reply", text_truncated: false };
  const result = instagramCommentLanguagePlan("creator", igSource).decode([post, reply]);
  assert.equal("author_id" in result, false); assert.equal("text" in result, false);
  assert.throws(() => instagramCommentLanguagePlan("creator", igSource).decode([post, { ...reply, text_truncated: true }]));
});
test("公开网页只接受原CLI固定来源封套，挑战页与坏UTF8明确失败", async () => {
  const requested = "https://creator.example/contact";
  const stdout = `# Contact\n\n> 原文链接: ${requested}\n\n---\n\n${"中".repeat(8000)}`;
  const diagnostic = Buffer.from(`[web-read diagnose]\nurl: ${requested}\n`);
  const result = await decodePublicWebRead(requested, stdout, diagnostic);
  assert.equal(result.markdown_excerpt, "中".repeat(6666)); assert.equal(result.truncated, true);
  await assert.rejects(decodePublicWebRead(requested, stdout, Buffer.from([255])));
  await assert.rejects(decodePublicWebRead(requested, stdout, Buffer.concat([diagnostic, diagnostic])));
  await assert.rejects(decodePublicWebRead(requested, stdout.replace("# Contact", "# Just a moment..."), diagnostic), { code: "opencli_risk_control" });
  await assert.rejects(decodePublicWebRead(requested, stdout.replace(`> 原文链接: ${requested}`, "> 原文链接: https://other.example/"), diagnostic));
});
test("Creator profile证据只接受原账户对应定位，不恢复退役来源", () => {
  assert.equal(creatorProfileSource("instagram", "123", "https://www.instagram.com/creator/"), "creator");
  assert.equal(creatorProfileSource("twitter", "creator", "https://x.com/creator"), "creator");
  assert.throws(() => creatorProfileSource("facebook", "123", "https://www.facebook.com/profile.php?id=456"));
  assert.throws(() => creatorProfileSource("twitter", "creator", "https://x.com/creator?x=1"));
  assert.throws(() => creatorProfileSource("hackernews", "author", "https://news.ycombinator.com/user?id=author"), { code: "opencli_read_command_unavailable" });
});
