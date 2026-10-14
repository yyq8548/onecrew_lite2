import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "./local-runtime";
import { discoveryReviewKey, getDiscoveryReview, reviewRefreshInterval, evaluateDiscoveryMatch, initialMatchHit, initialMatchReason, type ReviewMaterial } from "./discovery-review";
import { reviewFixture, reviewRequest, tiktokReviewFixture, xReviewFixture } from "./discovery-review.fixture";

vi.mock("./local-runtime", () => ({ invoke: vi.fn() }));
describe("saved discovery review contract", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());
  it("defers a pre-submission model failure during automatic scoring instead of stopping completed reads", async () => {
    const deferred = vi.fn();
    vi.mocked(invoke).mockRejectedValueOnce("initial_match_discovery_draft_plan_invalid");
    expect(await evaluateDiscoveryMatch({ campaign_id: "campaign", expected_revision: 1, search_hit_id: "hit" }, false, deferred)).toBeNull();
    expect(deferred).toHaveBeenCalledWith("initial_match_discovery_draft_plan_invalid");
    expect(invoke).toHaveBeenCalledTimes(1);
  });
  it("does not retry a failed model evaluation or invent a result without bound evidence", async () => {
    const input = { campaign_id: "campaign", expected_revision: 1, search_hit_id: "hit" };
    vi.mocked(invoke).mockResolvedValueOnce(null);
    expect(await evaluateDiscoveryMatch(input)).toBeNull();
    vi.mocked(invoke).mockRejectedValueOnce(new Error("initial_match_codex_failed"));
    await expect(evaluateDiscoveryMatch(input)).rejects.toThrow("initial_match_codex_failed");
    expect(invoke).toHaveBeenCalledTimes(2);
    expect(initialMatchReason({ policy_version: "initial-match-evaluation-v1", reason: "Missing original text" })).toBe("Missing original text");
    expect(initialMatchReason({ policy_version: "legacy", reason: "Old checklist" })).toBeNull();
  });
  it.each(["initial_match_busy", "initial_match_discovery_draft_codex_unavailable",
    "initial_match_discovery_draft_codex_login_required", "initial_match_discovery_draft_codex_failed",
    "initial_match_discovery_draft_codex_timeout", "initial_match_discovery_draft_plan_invalid"])("defers only the pre-submission error %s", async code => {
    const input = { campaign_id: "campaign", expected_revision: 1, search_hit_id: "hit" };
    const deferred = vi.fn();
    vi.mocked(invoke).mockRejectedValueOnce(new Error(code));
    expect(await evaluateDiscoveryMatch(input, false, deferred)).toBeNull();
    expect(deferred).toHaveBeenCalledWith(code);
    vi.mocked(invoke).mockRejectedValueOnce(new Error(code));
    await expect(evaluateDiscoveryMatch(input)).rejects.toThrow(code);
  });
  it.each(["local_runtime_request_failed", "initial_match_account_changed", "discovery_revision_changed",
    "initial_match_material_unavailable", "discovery_review_submission_failed"])("still stops on uncertain or changed state: %s", async code => {
    const deferred = vi.fn();
    vi.mocked(invoke).mockRejectedValueOnce(code);
    await expect(evaluateDiscoveryMatch({ campaign_id: "campaign", expected_revision: 1, search_hit_id: "hit" }, false, deferred)).rejects.toBe(code);
    expect(deferred).not.toHaveBeenCalled();
  });
  it("does not hide a manual reassessment failure or an invalid review receipt", async () => {
    const input = { campaign_id: "campaign", expected_revision: 1, search_hit_id: "hit" };
    const deferred = vi.fn();
    vi.mocked(invoke).mockRejectedValueOnce("initial_match_discovery_draft_plan_invalid");
    await expect(evaluateDiscoveryMatch(input, true, deferred)).rejects.toBe("initial_match_discovery_draft_plan_invalid");
    vi.mocked(invoke).mockResolvedValueOnce({});
    await expect(evaluateDiscoveryMatch(input, false, deferred)).rejects.toThrow("initial_match_receipt_invalid");
    expect(deferred).not.toHaveBeenCalled();
  });
  it("scores completed author material and skips failed reads, searches and identity-only reads", () => {
    const base = { campaign: {}, counts: {}, profile: null, content: null, identity: null, transcript: null };
    const result = (step: unknown, extra = {}) => ({ ...base, step, ...extra }) as Parameters<typeof initialMatchHit>[0];
    expect(initialMatchHit(result({ kind: "read_video", search_hit_id: "hit", profile_ref: "profile" }))).toBe("hit");
    expect(initialMatchHit(result({ kind: "read_video", search_hit_id: "hit", profile_ref: "profile" }, { provider_stop_code: "opencli_timeout" }))).toBeNull();
    expect(initialMatchHit(result({ kind: "read_profile", search_hit_id: "hit" }, { profile: { status: "completed" } }))).toBe("hit");
    expect(initialMatchHit(result({ kind: "read_content", search_hit_id: "hit" }, { content: { status: "failed" } }))).toBeNull();
    expect(initialMatchHit(result({ kind: "search", platform: "youtube", profile_ref: "profile" }))).toBeNull();
    expect(initialMatchHit(result({ kind: "read_identity", search_hit_id: "hit" }))).toBeNull();
  });
  it("reads the saved owner snapshot without running a provider or submitting a verdict", async () => {
    const packet = reviewFixture();
    vi.mocked(invoke).mockResolvedValue(packet);
    expect(await getDiscoveryReview(reviewRequest)).toEqual(packet);
    expect(vi.mocked(invoke).mock.calls).toEqual([["onecrew_discovery_review_material_get", {
      input: { campaign_id: "campaign", expected_revision: 1, search_hit_id: "hit" },
    }]]);
  });
  it.each(["google", "yahoo"])("accepts original source material under %s approval without inventing native search authority", async origin => {
    const author = "UCabcdefghijklmnopqrstuv";
    const packet = { ...reviewFixture(), format_version: 2, native_platform: "youtube", native_author_id: author,
      video_description: "Original keyboard comparison", transcript: null, public_pages: [],
      approved_scopes: [{ platform: origin, profile_ref: "dedicated", keywords: ["AI tools"] }],
      hit: { ...reviewFixture().hit, platform: origin, author_id: author,
        author_url: `https://www.youtube.com/channel/${author}`, author_observed_at: 1 } };
    vi.mocked(invoke).mockResolvedValueOnce(packet);
    expect((await getDiscoveryReview(reviewRequest)).hit.platform).toBe(origin);
    for (const changed of [
      { ...packet, native_platform: "tiktok" },
      { ...packet, video_description: null },
      { ...packet, hit: { ...packet.hit, author_id: "other" } },
      { ...packet, approved_scopes: [{ platform: "youtube", profile_ref: "dedicated", keywords: ["AI tools"] }] },
    ]) {
      vi.mocked(invoke).mockResolvedValueOnce(changed);
      await expect(getDiscoveryReview(reviewRequest)).rejects.toThrow();
    }
  });
  it.each(["google", "yahoo"])("accepts bound X original and profile under %s origin, rejects wrong author and source", async origin => {
    const author="987654321012345678";
    const profile="https://x.com/creator_1";
    const packet={...reviewFixture(),format_version:2,native_platform:"twitter",native_author_id:author,
      native_followers:1500,native_profile:{profile_url:profile,observed_at:1,bio:"Business studio@example.org"},
      video_description:"Original keyboard review.",transcript:null,public_pages:[],
      approved_scopes:[{platform:origin,profile_ref:"dedicated",keywords:["AI tools"]}],
      hit:{...reviewFixture().hit,platform:origin,source_url:`${profile}/status/1234567890123456789`,author_id:author,author_url:profile,author_observed_at:1}};
    vi.mocked(invoke).mockResolvedValueOnce(packet);
    expect((await getDiscoveryReview(reviewRequest)).native_platform).toBe("twitter");
    vi.mocked(invoke).mockResolvedValueOnce({...packet,hit:{...packet.hit,source_url:"https://x.com/Creator_1/status/1234567890123456789"}});
    expect((await getDiscoveryReview(reviewRequest)).native_platform).toBe("twitter");
    for(const changed of [{...packet,native_author_id:"UCabcdefghijklmnopqrstuv"},
      {...packet,hit:{...packet.hit,author_url:"https://x.com/other"}},
      {...packet,hit:{...packet.hit,source_url:`${profile}/status/123?token=private`}},
      {...packet,native_profile:{...packet.native_profile,profile_url:"https://x.com/other"}},
      {...packet,approved_scopes:[{platform:"twitter",profile_ref:"dedicated",keywords:["AI tools"]}]}]) {
      vi.mocked(invoke).mockResolvedValueOnce(changed);
      await expect(getDiscoveryReview(reviewRequest)).rejects.toThrow();
    }
  });
  it.each(["google", "yahoo"])("accepts bound Douyin evidence from %s only", async origin => {
    const author="MS4wLjABAAAA12345678";
    const profile=`https://www.douyin.com/user/${author}`;
    const packet={...reviewFixture(),format_version:2,native_platform:"douyin",native_author_id:author,
      native_followers:1500,native_profile:{profile_url:profile,observed_at:1,bio:"Business studio@example.org"},
      video_description:"Original keyboard review.",transcript:null,public_pages:[],
      approved_scopes:[{platform:origin,profile_ref:"dedicated",keywords:["AI tools"]}],
      hit:{...reviewFixture().hit,platform:origin,source_url:"https://www.douyin.com/video/123",author_id:author,author_url:profile,author_observed_at:1}};
    vi.mocked(invoke).mockResolvedValueOnce(packet);
    expect((await getDiscoveryReview(reviewRequest)).native_platform).toBe("douyin");
    for(const changed of [{...packet,native_author_id:"MS4wLjABAAAAother123"},
      {...packet,hit:{...packet.hit,source_url:"https://www.douyin.com/video/123?token=private"}},
      {...packet,native_profile:{...packet.native_profile,profile_url:"https://www.douyin.com/user/other"}}]) {
      vi.mocked(invoke).mockResolvedValueOnce(changed);
      await expect(getDiscoveryReview(reviewRequest)).rejects.toThrow();
    }
  });
  it("accepts the bound Douyin original caption without requesting comments and rejects source/identity swaps", async () => {
    const author = "MS4wLjABAAAA12345678";
    const source = "https://www.douyin.com/video/123";
    const profile = `https://www.douyin.com/user/${author}`;
    const packet = { ...reviewFixture(), format_version: 2, video_description: null, transcript: null, public_pages: [],
      criteria: { language: "English" }, native_author_id: author, native_followers: null,
      native_profile: { profile_url: profile, observed_at: 1, bio: "Creator business@example.org" },
      approved_scopes: [{ platform: "douyin", profile_ref: "dedicated", keywords: ["AI tools"] }],
      hit: { ...reviewFixture().hit, platform: "douyin", source_url: source, author_id: author, author_url: profile, author_observed_at: 1 },
      native_content: { claim: { campaign_id: "campaign", expected_revision: 1, search_hit_id: "hit", platform: "douyin", profile_ref: "dedicated", author_id: author, source_url: source, call_number: 2 },
        status: "completed", stop_code: null, captured_at: 1, native_content: { post_id: "123", author_id: author, author_label: "Creator", profile_url: profile,
          source_url: source, text: "Original keyboard comparison", text_truncated: false, evidence_source: "visible_native_video_caption" } } };
    vi.mocked(invoke).mockResolvedValueOnce(packet);
    expect((await getDiscoveryReview(reviewRequest)).native_content?.claim.source_url).toBe(source);
    expect(invoke).toHaveBeenCalledTimes(1);
    for (const changed of [
      { ...packet, native_author_id: "MS4wLjABAAAAother123" },
      { ...packet, native_profile: { ...packet.native_profile, profile_url: "https://www.douyin.com/user/other" } },
      { ...packet, native_content: { ...packet.native_content, claim: { ...packet.native_content.claim, source_url: "https://www.douyin.com/video/456" } } },
      { ...packet, native_content: { ...packet.native_content, native_content: { ...packet.native_content.native_content, evidence_source: "recommendation" } } },
    ]) {
      vi.mocked(invoke).mockResolvedValueOnce(changed);
      await expect(getDiscoveryReview(reviewRequest)).rejects.toThrow();
    }
  });

  it("accepts only a bound native X post with a verified follower count", async () => {
    const packet = xReviewFixture();
    vi.mocked(invoke).mockResolvedValueOnce(packet);
    expect((await getDiscoveryReview(reviewRequest)).native_content).toEqual(packet.native_content);
    for (const changed of [
      { ...packet, native_followers: null },
      { ...packet, native_content: { ...packet.native_content!, claim: { ...packet.native_content!.claim, search_hit_id: "other" } } },
      { ...packet, native_content: { ...packet.native_content!, native_content: {
        ...packet.native_content!.native_content, identity: { ...("identity" in packet.native_content!.native_content! ? packet.native_content!.native_content.identity : {}), account_id: "other" },
      } } },
    ]) {
      vi.mocked(invoke).mockResolvedValueOnce(changed);
      await expect(getDiscoveryReview(reviewRequest)).rejects.toThrow();
    }
  });
  it("accepts a native TikTok author and saved caption, but rejects unbound evidence", async () => {
    const packet = tiktokReviewFixture();
    vi.mocked(invoke).mockResolvedValueOnce(packet);
    expect((await getDiscoveryReview(reviewRequest)).hit.title).toContain("dresses");
    for (const changed of [
      { ...packet, native_followers: null },
      { ...packet, hit: { ...packet.hit, author_id: "other" } },
      { ...packet, hit: { ...packet.hit, source_url: "https://www.tiktok.com/@other" } },
      { ...packet, hit: { ...packet.hit, title: "" } },
    ]) {
      vi.mocked(invoke).mockResolvedValueOnce(changed);
      await expect(getDiscoveryReview(reviewRequest)).rejects.toThrow();
    }
  });
  it("rejects invalid generated shapes", async () => {
    for (const raw of [null, {}, { ...reviewFixture(), criteria_revision: "1" }, { ...reviewFixture(), approved_at: 1.5 }]) {
      vi.mocked(invoke).mockResolvedValueOnce(raw);
      await expect(getDiscoveryReview(reviewRequest)).rejects.toThrow("discovery_review_invalid");
    }
  });
  const changes: [string, (m: ReviewMaterial) => void][] = [
    ["version", m => { m.format_version = 2; }], ["campaign", m => { m.campaign_id = "other"; }],
    ["revision", m => { m.criteria_revision++; }], ["hit", m => { m.hit.id = "other"; }],
    ["hit campaign", m => { m.hit.campaign_id = "other"; }], ["profile", m => { m.hit.profile_ref = "other"; }],
    ["platform", m => { m.hit.platform = "twitter"; }], ["scope", m => { m.approved_scopes[0].keywords = ["other"]; }],
    ["hash", m => { m.material_hash = "not-a-hash"; }], ["evidence hash", m => { m.candidate_evidence_hash = ""; }],
    ["native identity", m => { m.native_author_id = "other"; }], ["candidate", m => { m.candidate_id = "other"; }],
    ["transcript hit", m => { m.transcript!.search_hit_id = "other"; }],
    ["transcript source", m => { m.transcript!.source_url = "https://example.com/other"; }],
    ["page profile", m => { m.public_pages[0].profile_ref = "other"; }],
    ["duplicate page", m => { m.public_pages.push(m.public_pages[0]); }],
    ["page limit", m => { m.public_pages = Array.from({ length: 21 }, (_, i) => ({ ...m.public_pages[0], requested_url: `https://example.com/${i}` })); }],
  ];
  it.each(changes)("rejects changed %s", async (_label, change) => {
    const packet = reviewFixture(); change(packet);
    vi.mocked(invoke).mockResolvedValue(packet);
    await expect(getDiscoveryReview(reviewRequest)).rejects.toThrow("discovery_review_scope_changed");
    expect(invoke).toHaveBeenCalledTimes(1);
  });
  it("keeps query cache boundaries and limits local polling", () => {
    const key = discoveryReviewKey(reviewRequest);
    for (const patch of [{ campaignId: "other" }, { revision: 2 }, { hitId: "other" }, { profileRef: "other" }])
      expect(discoveryReviewKey({ ...reviewRequest, ...patch })).not.toEqual(key);
    expect(reviewRefreshInterval(0, false)).toBe(5000);
    expect(reviewRefreshInterval(11, false)).toBe(5000);
    expect(reviewRefreshInterval(12, false)).toBe(false);
    expect(reviewRefreshInterval(0, true)).toBe(false);
  });
});
