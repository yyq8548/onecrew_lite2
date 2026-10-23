import { beforeEach, describe, expect, it, vi } from "vitest";
import { parseDiscoveryContent, getDiscoveryContent } from "./opencli-discovery";
import { getDiscoveryReview } from "./discovery-review";
import { reviewFixture, reviewRequest } from "./discovery-review.fixture";
import { invoke } from "./local-runtime";

vi.mock("./local-runtime", () => ({ invoke: vi.fn() }));
const caption = (author: string, source: string, post = "456") => ({ author_id: author, post_id: post, source_url: source,
  text: "Original review text 日本語の説明 Contact: creator@example.org", text_truncated: false });
const account = "ACo1234567890abcdef";
const zhihu = "a".repeat(32);
const fixtures = [
  { platform: "linkedin", author: account, locator: "creator", profile: "https://www.linkedin.com/in/creator/",
    root: "https://www.linkedin.com/in/creator/", source: "https://www.linkedin.com/in/creator/",
    body: { account_id: account, author_handle: "creator", profile_url: "https://www.linkedin.com/in/creator/", post_id: "456",
      source_url: "https://www.linkedin.com/feed/update/urn:li:activity:456/", text: "Original Activity text", text_truncated: true, evidence_source: "visible_author_activity" } },
  { platform: "pixiv", author: "123", locator: "123", profile: "https://www.pixiv.net/users/123",
    root: "https://www.pixiv.net/artworks/456", source: "https://www.pixiv.net/artworks/456", body: caption("123", "https://www.pixiv.net/artworks/456") },
  { platform: "weibo", author: "123", locator: "123", profile: "https://weibo.com/u/123",
    root: "https://weibo.com/123/Abcd", source: "https://weibo.com/123/Abcd",
    body: { ...caption("123", "https://weibo.com/123/Abcd"), author: "Creator", text_completeness_known: true, is_repost: false } },
  { platform: "zhihu", author: zhihu, locator: zhihu, profile: "https://www.zhihu.com/people/creator",
    root: "https://www.zhihu.com/question/123/answer/456", source: "https://www.zhihu.com/question/123/answer/456",
    body: { author_id: zhihu, url_token: "creator", author_label: "Creator", author_url: "https://www.zhihu.com/people/creator",
      question_id: "123", answer_id: "456", source_url: "https://www.zhihu.com/question/123/answer/456", text: "Original answer", public_links: [] } },
  { platform: "instagram", author: "123", locator: "123", profile: "https://www.instagram.com/creator/",
    root: "https://www.instagram.com/creator/", source: "https://www.instagram.com/creator/", body: caption("123", "https://www.instagram.com/p/Original1/") },
  { platform: "facebook", author: "123", locator: "123", profile: "https://www.facebook.com/profile.php?id=123",
    root: "https://www.facebook.com/profile.php?id=123", source: "https://www.facebook.com/reel/456", body: caption("123", "https://www.facebook.com/reel/456") },
];

describe("native platform receipts reach the original-text review", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());
  it.each(fixtures)("accepts bound $platform original evidence without searching or requesting comments", async fixture => {
    const read = { claim: { campaign_id: "campaign", expected_revision: 1, search_hit_id: "hit", platform: fixture.platform,
      profile_ref: "dedicated", author_id: fixture.locator, source_url: fixture.source, call_number: 2 },
      status: "completed", native_content: fixture.body, stop_code: null, captured_at: 1 };
    const packet = { ...reviewFixture(), format_version: 2, native_author_id: fixture.author, native_followers: null,
      native_profile: { profile_url: fixture.profile, observed_at: 1, bio: "Public bio" },
      hit: { ...reviewFixture().hit, platform: fixture.platform, source_url: fixture.root, author_id: fixture.author, author_url: fixture.profile, author_observed_at: 1 },
      approved_scopes: [{ platform: fixture.platform, profile_ref: "dedicated", keywords: ["AI tools"] }],
      video_description: null, transcript: null, public_pages: [], native_content: read };
    expect(parseDiscoveryContent(read).native_content).toEqual(fixture.body);
    vi.mocked(invoke).mockResolvedValueOnce(packet);
    expect((await getDiscoveryReview(reviewRequest)).native_content?.native_content).toEqual(fixture.body);
    expect(invoke).toHaveBeenCalledExactlyOnceWith("onecrew_discovery_review_material_get", {
      input: { campaign_id: "campaign", expected_revision: 1, search_hit_id: "hit" },
    });
    for (const changed of [
      { ...packet, native_author_id: "different" },
      { ...packet, native_profile: { ...packet.native_profile, profile_url: "https://example.org/unrelated" } },
      { ...packet, native_content: { ...read, claim: { ...read.claim, profile_ref: "another-browser" } } },
      { ...packet, native_content: { ...read, claim: { ...read.claim, source_url: "https://example.org/other" } } },
      { ...packet, native_content: { ...read, native_content: { ...fixture.body, source_url: "https://example.org/other" } } },
      { ...packet, native_content: { ...read, native_content: { ...fixture.body, credentials: "unexpected" } } },
    ]) {
      vi.mocked(invoke).mockResolvedValueOnce(changed);
      await expect(getDiscoveryReview(reviewRequest)).rejects.toThrow();
    }
    vi.mocked(invoke).mockResolvedValueOnce({ ...read, status: "failed", native_content: null, stop_code: "opencli_invalid_output" });
    expect((await getDiscoveryContent("campaign", "hit")).status).toBe("failed");
  });

  it.each(["google", "yahoo"])("accepts only bound native-video comment receipts under %s origin authority", origin => {
    const author = "UCabcdefghijklmnopqrstuv";
    const source = "https://www.youtube.com/watch?v=abcdefghijk";
    const sample = { author_id: author, source_url: source, text: "Original keyboard comparison", comments: [
      { source_url: `${source}&lc=actual-1`, text: "Useful explanation of the switches" },
    ] };
    const read = { claim: { campaign_id: "campaign", expected_revision: 1, search_hit_id: "hit", platform: origin,
      profile_ref: "dedicated", author_id: author, source_url: source, call_number: 3 },
      status: "completed", native_content: null, language_sample: sample, stop_code: null, captured_at: 1 };
    expect(parseDiscoveryContent(read).claim.platform).toBe(origin);
    for (const changed of [
      { ...read, claim: { ...read.claim, author_id: "handle" } },
      { ...read, claim: { ...read.claim, source_url: `${source}&tracking=unbound` } },
      { ...read, native_content: caption(author, source) },
      { ...read, language_sample: { ...sample, author_id: "UCdifferentdifferentdiff" } },
      { ...read, language_sample: { ...sample, comments: [{ source_url: `${source}&lc=bad%3Did`, text: "Wrong native ID" }] } },
      { ...read, language_sample: { ...sample, comments: [{ source_url: "https://www.youtube.com/watch?v=lmnopqrstuv&lc=actual-1", text: "Another video" }] } },
      { ...read, language_sample: { ...sample, comments: [sample.comments[0], sample.comments[0]] } },
      { ...read, language_sample: { ...sample, comments: Array(6).fill(sample.comments[0]) } },
      { ...read, language_sample: { ...sample, text: "日".repeat(2667) } },
    ]) expect(() => parseDiscoveryContent(changed)).toThrow();
    expect(parseDiscoveryContent({ ...read, status: "failed", language_sample: undefined, stop_code: "opencli_risk_control" }).claim.platform).toBe(origin);
  });

  it("accepts LinkedIn's native ID and 16KB original text for conditional comments, never a profile handle or another post", () => {
    const source = "https://www.linkedin.com/feed/update/urn:li:activity:456/";
    const sample = { author_id: account, source_url: source, text: "a".repeat(16000), comments: [
      { source_url: `${source}#comment-1`, text: "" }, { source_url: `${source}#comment-2`, text: "日本語のコメント" },
    ] };
    const read = { claim: { campaign_id: "campaign", expected_revision: 1, search_hit_id: "hit", platform: "linkedin",
      profile_ref: "dedicated", author_id: account, source_url: source, call_number: 3 },
      status: "completed", native_content: null, language_sample: sample, stop_code: null, captured_at: 1 };
    expect(parseDiscoveryContent(read).language_sample).toEqual(sample);
    for (const changed of [
      { ...read, claim: { ...read.claim, author_id: "creator" }, language_sample: { ...sample, author_id: "creator" } },
      { ...read, language_sample: { ...sample, text: "a".repeat(16001) } },
      { ...read, language_sample: { ...sample, comments: [sample.comments[0], sample.comments[0]] } },
      { ...read, language_sample: { ...sample, comments: [{ source_url: "https://www.linkedin.com/feed/update/urn:li:activity:999/#comment-1", text: "他の投稿" }] } },
    ]) expect(() => parseDiscoveryContent(changed)).toThrow();
  });
});
