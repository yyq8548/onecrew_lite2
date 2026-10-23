import assert from "node:assert/strict";
import { test } from "node:test";
import { linkedinProfilePlan, linkedinContentPlan, linkedinContactPlan, linkedinCommentsPlan } from "../src/platforms/opencli/linkedin.ts";

const account = "ACoAAA123456789abcdefghijk";
test("LinkedIn资料严格保留原生账户、部分bio与精确粉丝，要求原前台窗口模式", () => {
  const source = "https://www.linkedin.com/in/creator/";
  const row = { adapter_version: "1.0.0", author_handle: "creator", account_id: account, author_label: "Creator", profile_url: source, follower_count: 1606, follower_text: "1,606 followers", headline: "Keyboard reviews", bio: "Business studio@example.org", public_links: ["https://creator.example/contact"], contact_links: [source + "overlay/contact-info/"], evidence_source: "visible_profile_header", bio_truncated: true };
  const plan = linkedinProfilePlan("creator"); assert.equal(plan.foregroundBrowser, true);
  assert.equal(plan.decode([row]).account_id, account);
  assert.equal(plan.decode([{ ...row, bio_truncated: false }]).bio_truncated, false);
  assert.throws(() => plan.decode([{ ...row, follower_count: 1600 }]));
  assert.throws(() => linkedinProfilePlan("self"));
});
test("LinkedIn内容、联系页及评论分别绑定作者账户与原生来源", () => {
  const source = "https://www.linkedin.com/feed/update/urn:li:activity:7480296707809804288/";
  const content = { account_id: account, author_handle: "creator", profile_url: "https://www.linkedin.com/in/creator/", post_id: "7480296707809804288", source_url: source, text: "Original visible post", text_truncated: true, evidence_source: "visible_author_activity" };
  assert.equal(linkedinContentPlan("creator", account).decode([content]).post_id, content.post_id);
  const comments = { author_handle: "creator", account_id: account, source_url: source, comments: [{ id: "7480303860733911040", parent_post_id: content.post_id, text: "" }], evidence_source: "visible_same_post_comments" };
  const plan = linkedinCommentsPlan("creator", account, source);
  assert.equal(plan.decode([comments]).length, 1);
  assert.throws(() => plan.decode([{ ...comments, comments: [...comments.comments, ...comments.comments] }]));
  const contact = { adapter_version: "1.0.0", author_handle: "creator", account_id: account, source_url: "https://www.linkedin.com/in/creator/overlay/contact-info/", title: "Contact", contact_text: "creator@example.org", contact_links: ["https://creator.example/contact"], truncated: true, evidence_source: "visible_contact_dialog" };
  assert.equal(linkedinContactPlan("creator", account).decode([contact]).markdown_excerpt, "Website: https://creator.example/contact\ncreator@example.org");
  assert.match(linkedinContactPlan("creator", account).decode([{ ...contact, contact_links: ["http://creator.example/"] }]).markdown_excerpt, /http:\/\/creator\.example\//);
  assert.throws(() => linkedinContactPlan("creator", account).decode([{ ...contact, contact_links: ["http://localhost/"] }]));
});
