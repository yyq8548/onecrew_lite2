import { describe, it, expect } from "vitest";
import { candidateConditions } from "./discovery-candidate-conditions";
import type { DiscoveryCandidate } from "./opencli-discovery";

const candidate: DiscoveryCandidate = { id: "candidate", platform: "youtube", platform_account_id: "author",
  display_name: "Name", profile_url: "https://www.youtube.com/channel/author", verdict: "pending",
  evidence: { source_urls: ["https://example.com/post"], assessment: {reasons: [], subscribers_normalized: 10000} } };

describe("manual candidate approval", () => {




  it("does not turn an absent language or category reason into a satisfied requirement", () => {
    expect(candidateConditions(candidate, { min_followers: 8000, language: "中文", category: "稳定币" }))
      .toEqual({met: ["followersMet"], reasons: ["language_unverified", "category_unverified"]});
  });
  it("keeps near-threshold and mismatching evidence unresolved", () => {
    const item = structuredClone(candidate);
    item.evidence.assessment!.reasons = ["subscriber_count_near_threshold", "language_mismatch"];
    expect(candidateConditions(item, {min_followers: 10000, language: "中文"}))
      .toEqual({met: [], reasons: ["subscriber_count_near_threshold", "language_mismatch"]});
  });
  it("requires the original author and selected language for a matched label", () => {
    const item = structuredClone(candidate);
    item.evidence.reviewed_language = { verified: true, native_author_id: "other", review_id: "review",
      source_url: "https://example.com/post", language: "中文", outcome: "supported" };
    expect(candidateConditions(item, {language: "中文"}).reasons).toEqual(["language_unverified"]);
    (item.evidence.reviewed_language as Record<string, unknown>).native_author_id = "author";
    expect(candidateConditions(item, {language: "中文"})).toEqual({met: ["languageMet"], reasons: []});
  });

});
