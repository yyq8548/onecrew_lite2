import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { randomUUID } from "node:crypto";
import { facebookProfilePlan, facebookContactPlan } from "../src/platforms/opencli/facebook-profile.ts";
import { linkedinContactPlan, linkedinContentPlan } from "../src/platforms/opencli/linkedin.ts";
import { assessDiscoveryProfile } from "../src/discovery/qualification.ts";
import { extractDiscoveryContacts } from "../src/discovery/contacts.ts";
import { tiktokProfilePlan } from "../src/platforms/opencli/profiles.ts";
import { applyDiscoveryReviewFindings } from "../src/discovery/review-findings.ts";
import { applyDiscoverySelectionQuality } from "../src/discovery/selection-quality.ts";

const receipts = JSON.parse(readFileSync(new URL("./fixtures/platform-optimization-20261019.json", import.meta.url), "utf8"));

test("真实 Google 转读 Instagram 的账号绑定作品预览支持主题匹配，简介和预览均不冒充作者语言证据", () => {
  const material = JSON.parse(readFileSync(new URL("./fixtures/google-instagram-native-review-20261019.json", import.meta.url), "utf8"));
  const bio = { kind: "native_profile" as const, profile_url: material.native_profile.profile_url, quote: "Makeup Artist\nMUA | Masshole | Pug Mom | Founder @povbeauty" };
  const preview = { kind: "native_profile" as const, profile_url: material.native_profile.profile_url, quote: "TRACE IT! Coming to @povbeauty Oct 22." };
  const category = { criterion: "category_content" as const, outcome: "supported" as const, rationale: "已核实账号简介为 Makeup Artist，作者发布的可见作品预览包含其品牌新品；仅用于确认美妆主题。", citations: [bio, preview] };
  const selection = { criterion: "selection_fit" as const, outcome: "supported" as const, rationale: "美妆专业身份与本人发布的作品预览共同支持需求匹配，未声称已核实完整正文或全部发布历史。", citations: [bio, preview] };
  const quality = { criterion: "quality_rating" as const, outcome: "supported" as const, match_level: "low" as const, rationale: "已保存的简介和短篇作品预览支持主题匹配，缺少深入内容和教学质量证据，质量评分为 low。", citations: [bio, preview] };
  const reviewId = randomUUID();
  const categoryEvidence = applyDiscoveryReviewFindings(material, [category], { source_urls: [material.hit.source_url, material.native_profile.profile_url] }, reviewId);
  const evidence = applyDiscoverySelectionQuality(material, [selection, quality], categoryEvidence, reviewId);
  assert.equal(assessDiscoveryProfile(material.criteria, evidence, material.native_author_id, material.native_followers).verdict, "qualified");
  assert.equal((evidence.reviewed_selection as Record<string, unknown>).outcome, "supported");
  assert.equal((evidence.reviewed_quality as Record<string, unknown>).level, "low");
  assert.throws(() => applyDiscoveryReviewFindings(material, [{ ...category, citations: [bio] }], {}, randomUUID()), /review_first_hand_evidence_required/);
  assert.throws(() => applyDiscoveryReviewFindings({ ...material, criteria: { ...material.criteria, language: "English" } }, [{ criterion: "content_language", outcome: "supported", rationale: "边界验证：作品预览不足以核实作者语言，本项应被拒绝。", citations: [preview] }], {}, randomUUID()), /review_first_hand_evidence_required/);
});

test("真实 Facebook 数字主页和联系页采用实际目录地址，缺失简介和圆整粉丝分别保存", () => {
  const row = receipts.facebook_profile;
  assert.deepEqual(facebookProfilePlan(row.id).decode([row]), row);
  const page = facebookContactPlan(row.id, undefined, row.contact_links[0]).decode([receipts.facebook_contact]);
  assert.equal(page.document_url, row.contact_links[0]);
  assert.match(page.markdown_excerpt, /Messenger/);
  const contacts = extractDiscoveryContacts(page.markdown_excerpt, page.document_url, row.id, 1792454400, "author_published");
  assert.deepEqual(contacts.filter(contact => contact.kind === "messenger").map(contact => contact.normalized_value), receipts.facebook_contact.contact_links);
  assert.equal(row.bio_status, "missing");
  assert.equal(row.follower_count, null);
});

test("真实 LinkedIn 网站保存 HTTP 和未提供协议的原文", () => {
  for (const row of [receipts.linkedin_http_contact, receipts.linkedin_bare_contact]) {
    const page = linkedinContactPlan(row.author_handle, row.account_id).decode([row]);
    for (const link of row.contact_links) assert.ok(page.markdown_excerpt.includes(link));
    if (row.contact_text) assert.ok(page.markdown_excerpt.includes(row.contact_text));
    assert.ok(!page.markdown_excerpt.includes("https://www.christinaflach.com"));
    const websites = extractDiscoveryContacts(page.markdown_excerpt, page.document_url, row.account_id, 1792454400, "author_published").filter(contact => contact.kind === "website");
    assert.equal(websites.length, row === receipts.linkedin_http_contact ? 3 : 2);
    for (const website of websites) assert.ok(page.markdown_excerpt.includes(website.normalized_value));
  }
});

test("真实 LinkedIn 动态摘录保持作者和帖子身份及截断状态", () => {
  const row = receipts.linkedin_content_excerpt;
  assert.deepEqual(linkedinContentPlan(row.author_handle, row.account_id).decode([row]), row);
});

test("真实 TikTok 主页保留作者 Beacons 入口，捐款页和未核实短链接不算联系方式", () => {
  const row = receipts.tiktok_profile;
  assert.deepEqual(tiktokProfilePlan(row.username, row.id).decode([row]), row);
  const contacts = extractDiscoveryContacts(row.contact_links.map((link: string) => `Website: ${link}`).join("\n"), row.profile_url, row.id, 1792454400, "author_published");
  assert.deepEqual(contacts.map(contact => contact.original_value), row.contact_links);
  for (const item of receipts.tiktok_other_links) assert.equal(extractDiscoveryContacts(`Website: ${item.link}`, item.profile_url, item.author_id, 1792454400, "author_published").length, 0);
});

test("真实 X 审核的 medium 满足其他条件后进入推荐", () => {
  const row = receipts.role_medium_review;
  const result = assessDiscoveryProfile(row.criteria, row.evidence, row.author_id, row.followers);
  assert.equal(result.verdict, "qualified");
  assert.deepEqual(result.reasons, []);
  const constrained = assessDiscoveryProfile({ ...row.criteria, min_subscribers: row.followers + 1 }, row.evidence, row.author_id, row.followers);
  assert.equal(constrained.verdict, "rejected");
  assert.ok(constrained.reasons.includes("below_min_subscribers"));
});
