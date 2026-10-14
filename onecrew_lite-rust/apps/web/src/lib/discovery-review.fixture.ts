import type { ReviewMaterial, ReviewRequest } from "./discovery-review";

export const reviewRequest: ReviewRequest = { campaignId: "campaign", revision: 1, hitId: "hit", profileRef: "dedicated" };
export function reviewFixture(): ReviewMaterial {
  const source = "https://www.youtube.com/watch?v=abcdef12345";
  const binding = { search_hit_id: "hit", candidate_id: "candidate", source_url: source,
    profile_ref: "dedicated", platform_account_id: "channel", call_number: 3, captured_at: 1 };
  return {
    format_version: 1, campaign_id: "campaign", criteria_revision: 1,
    criteria: { requires_ai_tool_demo: true }, approved_at: 1,
    approved_scopes: [{ platform: "youtube", profile_ref: "dedicated", keywords: ["AI tools"] }],
    candidate_id: "candidate", native_author_id: "channel", candidate_evidence_hash: "a".repeat(64), material_hash: "b".repeat(64),
    hit: { id: "hit", campaign_id: "campaign", platform: "youtube", profile_ref: "dedicated",
      search_keyword: "AI tools", source_url: source, title: "Observed demo", author_label: "Creator", captured_at: 1,
      author_id: null, author_url: null, author_observed_at: null },
    video_description: "<script>description</script>",
    transcript: { ...binding, status: "completed", segments: [{ timestamp: "00:01", speaker: "Creator", text: "<script>transcript</script>" }], truncated: true, stop_code: null },
    public_pages: [{ ...binding, requested_url: "https://example.com/tool", source_excerpt: "Creator description",
      status: "completed", document_url: "https://example.com/tool", title: "Observed tool", markdown_excerpt: "[unsafe](javascript:alert(1))",
      truncated: true, stop_code: null }],
  };
}

export function xReviewFixture(): ReviewMaterial {
  const source = "https://x.com/creator_1/status/123456789";
  return {
    ...reviewFixture(), format_version: 2,
    criteria: { requires_ai_tool_demo: false, requires_public_tool_url: false,
      category: "服装", region: "中国大陆以外", min_followers: 10000 },
    approved_scopes: [{ platform: "twitter", profile_ref: "dedicated", keywords: ["apparel export"] }],
    hit: { id: "hit", campaign_id: "campaign", platform: "twitter", profile_ref: "dedicated",
      search_keyword: "apparel export", source_url: source, title: "Clothing brand", author_label: "Creator",
      captured_at: 1, author_id: "creator_1", author_url: "https://x.com/creator_1", author_observed_at: 1 },
    native_author_id: "987654321", native_followers: 15000,
    native_profile: { profile_url: "https://x.com/creator_1", observed_at: 1,
      bio: "New York apparel founder", self_declared_location: "New York, NY" },
    video_description: null, transcript: null, public_pages: [],
    native_content: { claim: { campaign_id: "campaign", expected_revision: 1, search_hit_id: "hit",
      platform: "twitter", profile_ref: "dedicated", author_id: "creator_1", source_url: source, call_number: 4 },
      status: "completed", stop_code: null, captured_at: 1,
      native_content: { adapter_version: "1.0.0", identity: { adapter_version: "1.0.0", post_id: "123456789",
        account_id: "987654321", handle: "creator_1", profile_url: "https://x.com/creator_1", source_url: source,
        identity_kind: "native_post_author_id", evidence_source: "x_tweet_detail" },
        text: "I run a <script>clothing</script> brand selling internationally.", text_truncated: false,
        text_source: "legacy_full_text", created_at: "2026-09-28", has_media: false,
        quotes_another_post: false, expanded_urls: [] } },
  };
}

export function tiktokReviewFixture(): ReviewMaterial {
  const source = "https://www.tiktok.com/@maker/video/7326242178584333610";
  const { native_content: _nativeContent, ...material } = xReviewFixture();
  return {
    ...material,
    approved_scopes: [{ platform: "tiktok", profile_ref: "dedicated", keywords: ["apparel export"] }],
    hit: { ...material.hit, platform: "tiktok", source_url: source,
      title: "I sell <script>dresses</script> to international wholesale buyers.",
      author_id: "9876543210123456789", author_url: "https://www.tiktok.com/@maker" },
    native_author_id: "9876543210123456789",
    native_profile: { profile_url: "https://www.tiktok.com/@maker", observed_at: 1,
      bio: "Wholesale apparel" },
  };
}
