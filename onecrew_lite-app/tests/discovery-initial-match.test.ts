import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { z } from "zod";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { applyInitialDiscoveryMatch, type DiscoveryReviewFinding } from "../src/discovery/initial-match.ts";
import { buildDiscoveryReviewMaterial } from "../src/discovery/review-material.ts";

function fixture() {
  const original = z.array(z.object({ basis: z.unknown() })).parse(parseApplicationJson(readFileSync(new URL("./fixtures/discovery-review-rust.json", import.meta.url))));
  const material = buildDiscoveryReviewMaterial(original[0]!.basis), source = "https://x.com/creator/status/123";
  material.hit = { ...material.hit, platform: "twitter", source_url: source, author_id: "100", author_url: "https://x.com/creator", author_observed_at: 1 };
  material.criteria = { language: "日语", min_followers: 8000 };
  material.native_author_id = "100"; material.native_followers = 10000; delete material.native_follower_estimate; delete material.native_content; delete material.native_profile; material.transcript = null; material.video_description = null;
  const comments = ["この説明はとても分かりやすいです", "この製品を使ってみたいです", "紹介してくれてありがとうございます", "Thanks for sharing", "Interesting product"].map((text, index) => ({ source_url: `https://x.com/viewer/status/${200 + index}`, text }));
  material.language_samples = [{ author_id: "100", source_url: source, text: "Original English content", comments }];
  const finding: DiscoveryReviewFinding = { criterion: "match_fit", outcome: "supported", rationale: "粉丝评论语言符合：日语；作者语言未核实", match_level: "good", citations: comments.slice(0, 3).map(comment => ({ kind: "native_content", quote: comment.text })) };
  return { material, finding };
}

test("初次评分评论语言仅接受同帖前五条三份不同完整引用，不能以此评excellent或证明口语", () => {
  const { material, finding } = fixture();
  const proof = applyInitialDiscoveryMatch(material, [finding], { source_urls: [material.hit.source_url] }, "review").reviewed_match;
  assert.deepEqual(proof, { verified: true, native_author_id: "100", source_url: material.hit.source_url, review_id: "review", material_hash: material.material_hash, policy_version: "initial-match-evaluation-v1", outcome: "supported", level: "good", reason: finding.rationale, language_only_pending: false });
  assert.throws(() => applyInitialDiscoveryMatch(material, [{ ...finding, citations: finding.citations.slice(0, 2) }], {}, "review"), /audience_sample_insufficient/);
  assert.throws(() => applyInitialDiscoveryMatch(material, [{ ...finding, match_level: "excellent" }], {}, "review"), /audience_language_invalid/);
  material.criteria = { language: "日语口语", min_followers: 8000 };
  assert.throws(() => applyInitialDiscoveryMatch(material, [finding], {}, "review"), /audience_language_invalid/);
});

test("初次评分必需粉丝失败优先，未知计数不能产生非low已支持结论", () => {
  const { material, finding } = fixture(); material.native_followers = 7999;
  assert.throws(() => applyInitialDiscoveryMatch(material, [finding], {}, "review"), /required_failure_must_be_low/);
  assert.equal((applyInitialDiscoveryMatch(material, [{ ...finding, match_level: "low", citations: [] }], {}, "review").reviewed_match as Record<string, unknown>).level, "low");
  delete material.native_followers;
  assert.throws(() => applyInitialDiscoveryMatch(material, [finding], {}, "review"), /required_count_unknown/);
  material.native_follower_estimate = { display: "0.8万", nominal: 8000, lower: 7500, upper: 8500 };
  assert.throws(() => applyInitialDiscoveryMatch(material, [finding], {}, "review"), /required_count_unknown/);
  assert.throws(() => applyInitialDiscoveryMatch(material, [{ ...finding, citations: [{ kind: "native_content", quote: "invented" }] }], {}, "review"), /initial_match_citation_not_saved/);
});
