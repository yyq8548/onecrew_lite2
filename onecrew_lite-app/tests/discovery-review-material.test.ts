import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../src/contracts/application-json.ts";
import { buildDiscoveryReviewMaterial, containsDiscoveryCitation, type DiscoveryReviewMaterial } from "../src/discovery/review-material.ts";

test("原Rust真实review仓储材料与transcript/page收据的全部字段、hash和引用匹配", () => {
  const cases = z.array(z.object({ basis: z.unknown(), expected: z.unknown() })).parse(parseApplicationJson(readFileSync(new URL("./fixtures/discovery-review-rust.json", import.meta.url))));
  assert.equal(cases.length, 2);
  for (const value of cases) assert.deepEqual(parseApplicationJson(Buffer.from(stringifyApplicationJson(buildDiscoveryReviewMaterial(value.basis)))), value.expected);
  const material = buildDiscoveryReviewMaterial(cases[1]!.basis);
  assert.equal(containsDiscoveryCitation(material, { kind: "transcript", segment_index: 0, quote: "I type a prompt." }), true);
  assert.equal(containsDiscoveryCitation(material, { kind: "transcript", segment_index: 0, quote: "invented text" }), false);
  assert.equal(containsDiscoveryCitation(material, { kind: "public_page", requested_url: "https://example.com/tool", quote: "AI editing software." }), true);
  assert.equal(containsDiscoveryCitation(material, { kind: "public_page", requested_url: "https://example.com/other", quote: "AI editing software." }), false);
});

test("真实Bilibili原视频标题可引用，搜索标题和其他作者或来源不能替代原生回执", () => {
  // 来自2026-10-21调用记录的owner_receipts.materials[5d4c422f-800d-4c46-aed7-b9ca31dceae4]。
  const material: DiscoveryReviewMaterial = {
    format_version: 2, campaign_id: "crypto-platform-test-20261021", criteria_revision: 1,
    criteria: { automatic_query_expansion: true, category: "虚拟货币", contact_preference: "public_email_first", min_subscribers: 2000, min_subscribers_source: "default", requires_ai_tool_demo: false, requires_public_tool_url: false },
    approved_scopes: [
      { platform: "youtube", profile_ref: "d5pskwub", keywords: ["cryptocurrency", "bitcoin", "crypto analysis", "blockchain"] },
      { platform: "tiktok", profile_ref: "d5pskwub", keywords: ["cryptocurrency", "bitcoin", "crypto analysis", "blockchain"] },
      { platform: "instagram", profile_ref: "d5pskwub", keywords: ["cryptocurrency", "bitcoin", "crypto analysis", "blockchain"] },
      { platform: "twitter", profile_ref: "d5pskwub", keywords: ["cryptocurrency", "bitcoin", "crypto analysis", "blockchain"] },
      { platform: "linkedin", profile_ref: "d5pskwub", keywords: ["cryptocurrency", "bitcoin", "crypto analysis", "blockchain"] },
      { platform: "facebook", profile_ref: "d5pskwub", keywords: ["cryptocurrency", "bitcoin", "crypto analysis", "blockchain"] },
      { platform: "bilibili", profile_ref: "d5pskwub", keywords: ["加密货币", "比特币", "币圈", "以太坊"] },
      { platform: "douyin", profile_ref: "d5pskwub", keywords: ["加密货币", "比特币", "币圈", "以太坊"] },
      { platform: "weibo", profile_ref: "d5pskwub", keywords: ["加密货币", "比特币", "币圈", "以太坊"] },
      { platform: "google", profile_ref: "d5pskwub", keywords: ["cryptocurrency", "bitcoin", "crypto analysis", "blockchain"] },
      { platform: "yahoo", profile_ref: "d5pskwub", keywords: ["cryptocurrency", "bitcoin", "crypto analysis", "blockchain"] },
    ],
    approved_at: 1792584126,
    hit: { id: "5d4c422f-800d-4c46-aed7-b9ca31dceae4", campaign_id: "crypto-platform-test-20261021", platform: "bilibili", profile_ref: "d5pskwub", search_keyword: "加密货币", source_url: "https://www.bilibili.com/video/BV1T5MEzNE47", title: "【从0到1】币圈新手入门终极指南", author_label: "妙手缠君", captured_at: 1792588279, author_id: "3546756316923987", author_url: "https://space.bilibili.com/3546756316923987", author_observed_at: 1792588279 },
    candidate_id: "53757084-da29-4cb4-ad70-4a933fe68ced", native_author_id: "3546756316923987", native_followers: 15747,
    native_profile: { profile_url: "https://space.bilibili.com/3546756316923987", observed_at: 1792588291, bio: "公粽——缠论妙手", bio_status: "complete" },
    candidate_evidence_hash: "26782e3aefca04e2745af162c3fa9e47b9f6d5ee76c1a9bcfbf767b4c97c5700", video_description: null, transcript: null,
    native_content: {
      claim: { campaign_id: "crypto-platform-test-20261021", expected_revision: 1, search_hit_id: "5d4c422f-800d-4c46-aed7-b9ca31dceae4", platform: "bilibili", profile_ref: "d5pskwub", author_id: "3546756316923987", source_url: "https://www.bilibili.com/video/BV1T5MEzNE47", call_number: 200 },
      status: "completed", stop_code: null, captured_at: 1792588298,
      native_content: {
        author: "妙手缠君", author_id: "3546756316923987", bvid: "BV1T5MEzNE47", description: "愿在路上的你，前方不再伸手不见五指", description_truncated: false,
        observations: { comment_count: 2572, like_count: 4004, published_at: "2025-06-11T05:30:02.000Z", save_count: null, share_count: null, view_count: 264612 },
        pay_preview: false, payment_status_known: false, requires_payment: false, source_url: "https://www.bilibili.com/video/BV1T5MEzNE47", title: "【从0到1】币圈新手入门终极指南",
      },
    },
    public_pages: [], material_hash: "79cbf58ca39160cdaee18ffaedec0c4859ff33be78f3b11d407cee22460d239b",
  };
  const citation = { kind: "native_content" as const, quote: "【从0到1】币圈新手入门终极指南" }, read = material.native_content!;
  assert.equal(containsDiscoveryCitation(material, citation), true);
  assert.equal(containsDiscoveryCitation(material, { kind: "native_content", quote: "愿在路上的你，前方不再伸手不见五指" }), true);
  const searchOnly = { ...material }; delete searchOnly.native_content;
  assert.equal(containsDiscoveryCitation(searchOnly, citation), false);
  assert.equal(containsDiscoveryCitation(material, { kind: "search_caption", quote: citation.quote }), false);
  const scale = { kind: "native_profile" as const, profile_url: material.native_profile!.profile_url, quote: String(material.native_followers) };
  assert.equal(containsDiscoveryCitation(material, scale), true);
  assert.equal(containsDiscoveryCitation(material, { ...scale, quote: "15748" }), false);
  assert.equal(containsDiscoveryCitation(material, { ...scale, quote: "粉丝数15747" }), false);
  assert.equal(containsDiscoveryCitation({ ...material, native_author_id: "520819684" }, scale), false);
  // 同一调用记录另一条真实Bilibili视频的作者与地址，用于检查交叉绑定。
  const otherAuthor = "520819684", otherSource = "https://www.bilibili.com/video/BV1SquEzGERq", native = z.record(z.string(), z.unknown()).parse(read.native_content);
  assert.equal(containsDiscoveryCitation({ ...material, native_author_id: otherAuthor }, citation), false);
  assert.equal(containsDiscoveryCitation({ ...material, hit: { ...material.hit, author_id: otherAuthor } }, citation), false);
  assert.equal(containsDiscoveryCitation({ ...material, hit: { ...material.hit, source_url: otherSource } }, citation), false);
  assert.equal(containsDiscoveryCitation({ ...material, native_content: { ...read, claim: { ...read.claim, author_id: otherAuthor } } }, citation), false);
  assert.equal(containsDiscoveryCitation({ ...material, native_content: { ...read, claim: { ...read.claim, source_url: otherSource } } }, citation), false);
  assert.equal(containsDiscoveryCitation({ ...material, native_content: { ...read, native_content: { ...native, author_id: otherAuthor } } }, citation), false);
  assert.equal(containsDiscoveryCitation({ ...material, native_content: { ...read, native_content: { ...native, source_url: otherSource } } }, citation), false);
});
