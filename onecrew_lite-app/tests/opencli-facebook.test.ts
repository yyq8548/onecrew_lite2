import assert from "node:assert/strict";
import { test } from "node:test";
import { facebookIdentityPlan, facebookProfilePlan, facebookContactPlan } from "../src/platforms/opencli/facebook-profile.ts";
import { facebookRecentPlan, facebookReelPlan, facebookLanguagePlan, facebookNativeOriginalPlan } from "../src/platforms/opencli/facebook-language.ts";

const base = "https://www.facebook.com/LinusTech";
const author = "123";
test("Facebook原生身份和资料验证保存的vanity来源及数字账户", () => {
  const identity = { adapter_version: "1.0.0", source_url: base, id: author, display_name: "Creator", profile_url: base, evidence_source: "native_profile_header_response" };
  assert.equal(facebookIdentityPlan(base).decode([identity]).id, author);
  assert.throws(() => facebookIdentityPlan("https://www.facebook.com/groups"));
  assert.throws(() => facebookIdentityPlan(base).decode([{ ...identity, profile_url: base + "Other" }]));
  const row = { adapter_version: "2.0.0", id: author, display_name: "Creator", follower_count: null, follower_text: "1.3M followers", profile_url: "https://www.facebook.com/profile.php?id=123", observed_profile_url: base, contact_links: [base + "/directory_contact_info"], evidence_source: "native_profile_header_response" };
  assert.equal(facebookProfilePlan(author, base).decode([row]).follower_count, null);
  assert.throws(() => facebookProfilePlan(author).decode([row]));
  assert.throws(() => facebookProfilePlan(author, base).decode([{ ...row, follower_count: 1300000 }]));
});
test("Facebook联系页保留观察链接并按字节截取，不跨作者页", () => {
  const source = base + "/directory_contact_info";
  const row = { adapter_version: "2.0.0", id: author, source_url: source, observed_profile_url: base, title: "Contact", contact_text: "中".repeat(10000), contact_links: ["http://creator.example/contact"], truncated: false };
  const result = facebookContactPlan(author, { base, source }).decode([row]);
  assert.equal(result.markdown_excerpt.startsWith("Website: http://creator.example/contact\n"), true);
  assert.equal(result.truncated, true); assert.ok(Buffer.byteLength(result.markdown_excerpt) <= 20000);
  assert.throws(() => facebookContactPlan(author, { base, source: "https://www.facebook.com/Other/directory_links" }));
});
test("Facebook原caption与语言评论分别保留作者绑定和来源，不使用翻译或空正文", () => {
  const source = "https://www.facebook.com/reel/2067046843941874";
  const post = { type: "POST", source_url: source, native_author_id: author, author_url: "https://www.facebook.com/profile.php?id=123", identity_source: "visible_numeric_profile_link", text: "中".repeat(3000), text_truncated: false };
  const comment = { type: "COMMENT", source_url: source + "?comment_id=101", text: "Original reply", text_truncated: false };
  const content = facebookReelPlan(author, source).decode([post]);
  assert.equal(content.text, "中".repeat(2666)); assert.equal(content.text_truncated, true);
  const sample = facebookLanguagePlan(author, source).decode([post, comment]);
  assert.equal(sample.comments[0]!.source_url, comment.source_url);
  assert.throws(() => facebookLanguagePlan(author, source).decode([post, comment, comment]));
  assert.throws(() => facebookReelPlan(author, source).decode([{ ...post, text: "" }]));
  assert.deepEqual(facebookRecentPlan(author, base).decode([{ author_id: author, source_url: source }]), [source]);
  assert.throws(() => facebookRecentPlan(author).decode([]));
  const native = facebookNativeOriginalPlan(author, base, source).decode([{ post_id: "2067046843941874", author_id: author, source_url: source, text: "Original", text_truncated: false }]);
  assert.equal(native.text, "Original");
});
