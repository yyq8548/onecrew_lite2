import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { z } from "zod";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { type DiscoveryReviewFinding } from "../src/discovery/initial-match.ts";
import { buildDiscoveryReviewMaterial } from "../src/discovery/review-material.ts";
import { applyDiscoverySelectionQuality } from "../src/discovery/selection-quality.ts";

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

test("独立匹配沿用评论数量和来源校验，评论语言通过不会指定质量分数", () => {
  const { material, finding } = fixture();
  const selection: DiscoveryReviewFinding = { ...finding, criterion: "selection_fit", match_level: null };
  for (const level of ["excellent", "good", "medium", "low", null] as const) {
    const quality: DiscoveryReviewFinding = { ...finding, criterion: "quality_rating", outcome: level === null ? "insufficient" : "supported", match_level: level, citations: level === null ? [] : [{ kind: "native_content", quote: material.language_samples![0]!.text }] };
    const applied = applyDiscoverySelectionQuality(material, [selection, quality], {}, "separate-review");
    assert.equal((applied.reviewed_selection as Record<string, unknown>).outcome, "supported");
    assert.equal((applied.reviewed_quality as Record<string, unknown>).level, level);
    assert.throws(() => applyDiscoverySelectionQuality(material, [{ ...selection, citations: selection.citations.slice(0, 2) }, quality], {}, "separate-review"), /audience_sample_insufficient/);
  }
});
