import { beforeEach, describe, expect, it, vi } from "vitest";
import { canRecoverDiscoverySearch, canSearchDiscovery, canVerifyDiscovery, discoveryScopeKeywords, getDiscoveryContent, getDiscoveryProfile, getDiscoveryTranscript, listDiscoveryCandidates, listDiscoveryHits, nextDiscoveryPage, runDiscoveryCampaign, searchDiscoveryOnce, syncDiscoveryLibrary, validCategoryScopes, validScopedTags, validV2exNodes, type DiscoveryCampaign } from "./opencli-discovery";
import { invoke } from "./local-runtime";
import { getDiscoveryPublicPage, advanceDiscoveryCampaign } from "./opencli-discovery";
import { increaseDiscoveryTarget, prepareDiscoveryDraft, listRecentDiscoveryCampaigns, validDiscoveryCampaignId } from "./opencli-discovery";
import { evaluateDiscoveryMatch, initialMatchHit } from "./discovery-review";

vi.mock("./local-runtime", () => ({ invoke: vi.fn() }));

it("accepts owner-valid named MCP campaign IDs and UUIDs, but rejects unsafe input", () => {
  for (const id of ["instagram-mouse-public-acceptance-20261011", "campaign_1", "a".repeat(128), "01a12621-9090-7330-aae1-4843f37f2fff"]) {
    expect(validDiscoveryCampaignId(id)).toBe(true);
  }
  for (const id of ["", " ", "a".repeat(129), "../campaign", "plan?x=1", "计划", "campaign\n"]) {
    expect(validDiscoveryCampaignId(id)).toBe(false);
  }
});

describe("Campaign local result pagination", () => {
  beforeEach(() => { vi.mocked(invoke).mockReset(); });

  it("reads at most five saved plan summaries without issuing a search or approval", async () => {
    const plan = { id: "saved", brief: "鞋子", phase: "paused", target_count: 100, used_calls: 22, updated_at: 1790810000 };
    vi.mocked(invoke).mockResolvedValueOnce([plan]);
    expect(await listRecentDiscoveryCampaigns()).toEqual([plan]);
    expect(invoke).toHaveBeenCalledExactlyOnceWith("onecrew_discovery_campaigns_recent", {});
    vi.mocked(invoke).mockResolvedValueOnce(Array(6).fill(plan));
    await expect(listRecentDiscoveryCampaigns()).rejects.toThrow();
    vi.mocked(invoke).mockResolvedValueOnce([{ ...plan, credentials: "secret" }]);
    await expect(listRecentDiscoveryCampaigns()).rejects.toThrow();
  });

  it("submits product planning directly through authenticated HTTP without clipboard or MCP", async () => {
    const input = { id: "stable-draft", brief: '鞋子"\n便宜时尚', profile_ref: "browser",
      opencli_version: "1.8.8", target_count: 30 };
    const output = { ...input, phase: "draft", revision: 1, criteria: { min_subscribers: 0 },
      used_calls: 0, scopes: [{ platform: "youtube", profile_ref: "browser", keywords: ["平价鞋测评"] }], platform_runs: [] };
    vi.mocked(invoke).mockResolvedValueOnce(output);
    expect((await prepareDiscoveryDraft(input)).brief).toBe(input.brief);
    expect(vi.mocked(invoke)).toHaveBeenCalledExactlyOnceWith("onecrew_discovery_draft_prepare", { input });
    vi.mocked(invoke).mockRejectedValueOnce("discovery_draft_outcome_unknown");
    await expect(prepareDiscoveryDraft(input)).rejects.toBe("discovery_draft_outcome_unknown");
    expect(vi.mocked(invoke)).toHaveBeenCalledTimes(2); // no automatic write retry
  });

  it("reads saved public-page states without a provider step and rejects mismatched receipts", async () => {
    const base = { candidate_id: "candidate", requested_url: "https://example.com/tool", search_hit_id: "hit",
      source_url: "https://www.youtube.com/watch?v=LR04bU_yV5k", source_excerpt: "Use https://example.com/tool",
      profile_ref: "dedicated", platform_account_id: "UC1234567890", call_number: 3, captured_at: 100 };
    for (const state of [
      { status: "reading", document_url: null, title: "", markdown_excerpt: "", truncated: false, stop_code: null },
      { status: "failed", document_url: null, title: "", markdown_excerpt: "", truncated: false, stop_code: "opencli_timeout" },
      { status: "completed", document_url: "https://example.com/tool", title: "Tool", markdown_excerpt: "Observed", truncated: true, stop_code: null },
    ]) {
      const receipt = { ...base, ...state };
      vi.mocked(invoke).mockResolvedValueOnce(receipt);
      expect(await getDiscoveryPublicPage("campaign", "candidate", base.requested_url)).toEqual(receipt);
    }
    expect(vi.mocked(invoke).mock.calls).toEqual(Array(3).fill(["onecrew_discovery_public_page_get",
      { input: { campaign_id: "campaign", candidate_id: "candidate", requested_url: base.requested_url } }]));
    for (const patch of [{ candidate_id: "other" }, { requested_url: "https://example.com/other" }, { stop_code: "unexpected" }]) {
      vi.mocked(invoke).mockResolvedValueOnce({ ...base, status: "reading", document_url: null,
        title: "", markdown_excerpt: "", truncated: false, stop_code: null, ...patch });
      await expect(getDiscoveryPublicPage("campaign", "candidate", base.requested_url)).rejects.toThrow();
    }
  });

  it("uses the saved last row ID and stops on a partial or empty page", () => {
    const page = Array.from({ length: 100 }, (_, i) => ({ id: `saved-${i}` }));
    expect(nextDiscoveryPage(page)).toBe("saved-99");
    expect(nextDiscoveryPage(page.slice(0, 99))).toBeUndefined();
    expect(nextDiscoveryPage([])).toBeUndefined();
  });

  it("loads hits and candidates after an exact cursor using local reads only", async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await listDiscoveryHits("campaign-1");
    await listDiscoveryHits("campaign-1", "last-hit");
    await listDiscoveryCandidates("campaign-1", "last-candidate");
    expect(vi.mocked(invoke).mock.calls).toEqual([
      ["onecrew_discovery_search_hits_list", { input: { campaign_id: "campaign-1", after_id: null, limit: 100 } }],
      ["onecrew_discovery_search_hits_list", { input: { campaign_id: "campaign-1", after_id: "last-hit", limit: 100 } }],
      ["onecrew_discovery_candidates_list", { input: { campaign_id: "campaign-1", after_id: "last-candidate", limit: 100 } }],
    ]);
  });

  it("uses the typed Campaign-to-Library command and validates its table link", async () => {
    const result = { campaign_id: "campaign-1", table_id: "tbl1",
      web_url: "http://127.0.0.1:8091/library/tbl1", total_hits: 5, inserted_rows: 2 };
    vi.mocked(invoke).mockResolvedValueOnce(result).mockResolvedValueOnce({ ...result, web_url: "not-a-url" });
    expect(await syncDiscoveryLibrary("campaign-1")).toEqual(result);
    expect(vi.mocked(invoke).mock.calls[0]).toEqual(["onecrew_discovery_library_sync", { campaignId: "campaign-1" }]);
    await expect(syncDiscoveryLibrary("campaign-1")).rejects.toThrow();
    vi.mocked(invoke).mockResolvedValueOnce({ ...result, campaign_id: "different-campaign" });
    await expect(syncDiscoveryLibrary("campaign-1")).rejects.toThrow("library_sync_campaign_mismatch");
  });

  it("reads native profile candidates without turning pending evidence into qualification", async () => {
    const rows = [
      { platform: "bilibili", url: "https://space.bilibili.com/123", source: "https://www.bilibili.com/video/BV1RYqHYnEw8" },
      { platform: "tiktok", url: "https://www.tiktok.com/@creator", source: "https://www.tiktok.com/@creator/video/7123456789012345678" },
    ].map(({ platform, url, source }) => ({
      id: platform, platform, platform_account_id: "123", display_name: "Same name",
      profile_url: url, verdict: "pending",
      evidence: { source_urls: [source, url], subscribers_raw: "10000", subscribers_source_url: url, captured_at: 100,
        assessment: { reasons: ["own_ai_tool_demo_unverified", "public_tool_url_unverified"], subscribers_normalized: 10000 } },
    }));
    vi.mocked(invoke).mockResolvedValueOnce(rows);
    expect(await listDiscoveryCandidates("campaign-1")).toEqual(rows);
    expect(invoke).toHaveBeenCalledExactlyOnceWith("onecrew_discovery_candidates_list", {
      input: { campaign_id: "campaign-1", after_id: null, limit: 100 },
    });
  });

  it("preserves observed authors and explicit unknowns without inferring identity", async () => {
    const base = { id: "hit", platform: "bilibili", source_url: "https://www.bilibili.com/video/BV1RYqHYnEw8",
      title: "demo", author_label: "Same name", search_keyword: "ecommerce", captured_at: 100, profile_ref: "dedicated" };
    const observed = { ...base, author_id: "8420618", author_url: "https://space.bilibili.com/8420618", author_observed_at: 101 };
    const unknown = { ...base, id: "old-hit", author_id: null, author_url: null, author_observed_at: null };
    vi.mocked(invoke).mockResolvedValueOnce([observed, unknown]);
    expect(await listDiscoveryHits("campaign-1")).toEqual([observed, unknown]);
    vi.mocked(invoke).mockResolvedValueOnce([base]);
    await expect(listDiscoveryHits("campaign-1")).rejects.toThrow();
    vi.mocked(invoke).mockResolvedValueOnce([{ ...observed, author_url: null }]);
    await expect(listDiscoveryHits("campaign-1")).rejects.toThrow();
  });
});

it("keeps V2EX node scopes separate from general search terms", () => {
  expect(discoveryScopeKeywords("youtube", "AI tools, demos", "python")).toEqual(["AI tools", "demos"]);
  expect(discoveryScopeKeywords("v2ex-node", "AI tools, demos", "python, javascript")).toEqual(["python", "javascript"]);
  expect(validV2exNodes(["python", "javascript"])).toBe(true);
  expect(validV2exNodes(["AI tools"])).toBe(false);
  expect(validV2exNodes(["-python"])).toBe(false);
  expect(validV2exNodes(["python", "python"])).toBe(false);
  expect(discoveryScopeKeywords("devto-tag", "AI tools", "python", { "devto-tag": "ai, webdev" })).toEqual(["ai", "webdev"]);
  expect(validScopedTags(["ai", "webdev"])).toBe(true);
  expect(validScopedTags(["ai", "AI"])).toBe(false);
  expect(discoveryScopeKeywords("juejin-hot", "AI tools", "python", {}, { "juejin-hot": "ai, frontend" })).toEqual(["ai", "frontend"]);
  expect(validCategoryScopes("juejin-hot", ["ai", "frontend"])).toBe(true);
  expect(validCategoryScopes("juejin-hot", ["marketing-sales"])).toBe(false);
  expect(validCategoryScopes("producthunt-posts", ["marketing-sales"])).toBe(true);
});

const approved: DiscoveryCampaign = {
  id: "campaign-1", phase: "active", revision: 2, brief: "AI tool creators",
  opencli_version: "1.8.8", criteria: { min_subscribers: 10000, requires_ai_tool_demo: true, requires_public_tool_url: true },
  target_count: 20, used_calls: 0,
  scopes: [{ platform: "youtube", profile_ref: "work-profile", keywords: ["AI tools"] }],
  platform_runs: [{ platform: "youtube", profile_ref: "work-profile", status: "unsearched", calls_used: 0, stop_code: null }],
};

describe("foreground automatic Campaign runner", () => {
  beforeEach(() => { vi.mocked(invoke).mockReset(); });
  it("raises the target through the typed HTTP owner and rejects wrong readbacks", async () => {
    const input = { campaign_id: approved.id, expected_revision: approved.revision,
      expected_target_count: approved.target_count, new_target_count: 200, decision_id: "target-200" };
    const updated = { ...approved, target_count: 200 };
    vi.mocked(invoke).mockResolvedValueOnce(updated);
    expect(await increaseDiscoveryTarget(input)).toEqual(updated);
    expect(vi.mocked(invoke).mock.calls[0]).toEqual(["onecrew_discovery_target_increase", { input }]);
    for (const patch of [{ id: "other" }, { revision: 3 }, { target_count: 20 }]) {
      vi.mocked(invoke).mockResolvedValueOnce({ ...updated, ...patch });
      await expect(increaseDiscoveryTarget(input)).rejects.toThrow("discovery_target_readback_mismatch");
    }
    // The owner returns the current state for an idempotent replay, including
    // a later separately approved target increase.
    vi.mocked(invoke).mockResolvedValueOnce({ ...updated, target_count: 300 });
    await expect(increaseDiscoveryTarget(input)).rejects.toThrow("discovery_target_readback_mismatch");
    const lower = {...input, new_target_count: 8, decision_id: "target-8"};
    vi.mocked(invoke).mockResolvedValueOnce(approved);
    await expect(increaseDiscoveryTarget(lower)).rejects.toThrow("discovery_target_readback_mismatch");
    vi.mocked(invoke).mockResolvedValueOnce({...approved, target_count: 8});
    expect((await increaseDiscoveryTarget(lower)).target_count).toBe(8);
  });
  const response = (kind: string, calls: number) => ({
    step: kind === "search" ? { kind, platform: "youtube", profile_ref: "work-profile" } : { kind },
    campaign: { ...approved, used_calls: calls },
    counts: { qualified: 0, pending: 30, rejected: 0 },
    transcript: null,
    profile: null,
    content: null,
    identity: null,
  });

  it("continues after a source-bound Facebook identity receipt and rejects unrelated identity evidence", async () => {
    const claim={campaign_id:approved.id,expected_revision:approved.revision,search_hit_id:"facebook-hit",profile_ref:"work-profile",source_url:"https://www.facebook.com/NativeCreator",call_number:1};
    const native={adapter_version:"1.0.0",source_url:claim.source_url,id:"100044572203205",display_name:"Native creator",profile_url:claim.source_url,evidence_source:"native_profile_header_response"};
    const receipt={claim,status:"completed",native_identity:native,stop_code:null,captured_at:100};
    const packet={...response("read_facebook_identity",1),step:{kind:"read_facebook_identity",search_hit_id:claim.search_hit_id},facebook_identity:receipt,
      campaign:{...approved,used_calls:1,scopes:[{platform:"facebook",profile_ref:"work-profile",keywords:["keyboard"]}],platform_runs:[]}};
    vi.mocked(invoke).mockResolvedValueOnce(packet).mockResolvedValueOnce({...packet,step:{kind:"needs_evidence"},facebook_identity:undefined});
    const onStep=vi.fn();
    expect(await runDiscoveryCampaign({...packet.campaign,used_calls:0},new AbortController().signal,onStep)).toBe("needs_evidence");
    expect(onStep).toHaveBeenCalledTimes(2);
    for(const changed of [
      {claim:{...claim,profile_ref:"another"}}, {claim:{...claim,call_number:2}}, {claim:{...claim,expected_revision:claim.expected_revision+1}},
      {native_identity:{...native,source_url:"https://www.facebook.com/Other"}},
      {native_identity:{...native,id:"NativeCreator"}}, {native_identity:{...native,cookie:"forbidden"}},
      {status:"failed"},
    ]) {
      vi.mocked(invoke).mockResolvedValueOnce({...packet,facebook_identity:{...receipt,...changed}});
      await expect(advanceDiscoveryCampaign(approved.id,approved.revision)).rejects.toThrow();
    }
    vi.mocked(invoke).mockResolvedValueOnce({...packet,step:{kind:"needs_evidence"}});
    await expect(advanceDiscoveryCampaign(approved.id,approved.revision)).rejects.toThrow();
  });

  it("scores the primary native author while retaining actual engine authority and skipping duplicate sources", async () => {
    const claim={campaign_id:approved.id,expected_revision:approved.revision,search_hit_id:"web-hit",origin_platform:"google",profile_ref:"work-profile",
      source_url:"https://www.youtube.com/watch?v=abcdefghijk&pp=source",target_platform:"youtube",video_id:"abcdefghijk",call_number:1};
    const native={adapterVersion:"1.0.0",videoId:claim.video_id,channelId:"UCabcdefghijklmnopqrstuv",sourceUrl:"https://www.youtube.com/watch?v=abcdefghijk",
      title:"Original review",author:"Native creator",subscribers:"Unknown",description:"Original caption",descriptionSource:"ytInitialPlayerResponse.videoDetails.shortDescription",
      displayDescription:"Translated UI",links:[],playabilityStatus:"OK"};
    const receipt={claim,status:"completed",native_author_id:native.channelId,candidate_id:"native-candidate",is_primary_source:true,native_material:native,stop_code:null,captured_at:100};
    const packet={...response("read_web_source",1),step:{kind:"read_web_source",search_hit_id:"web-hit"},web_source:receipt,
      campaign:{...approved,used_calls:1,scopes:[{platform:"google",profile_ref:"work-profile",keywords:["keyboard review"]}],platform_runs:[]}};
    vi.mocked(invoke).mockResolvedValueOnce(packet).mockResolvedValueOnce({...packet,step:{kind:"needs_evidence"},web_source:undefined});
    const onStep=vi.fn();expect(await runDiscoveryCampaign(approved,new AbortController().signal,onStep)).toBe("needs_evidence");
    expect(initialMatchHit(onStep.mock.calls[0][0])).toBe("web-hit");
    vi.mocked(invoke).mockResolvedValueOnce({...packet,web_source:{...receipt,is_primary_source:false}});
    expect(initialMatchHit(await advanceDiscoveryCampaign(approved.id,approved.revision))).toBeNull();
    for(const changed of [{claim:{...claim,profile_ref:"other"}},{claim:{...claim,expected_revision:approved.revision+1}},
      {claim:{...claim,origin_platform:"yahoo"}},{claim:{...claim,call_number:2}},
      {candidate_id:null},{native_material:{...native,videoId:"mnopqrstuvw"}},{native_material:{...native,channelId:"wrong"}},
      {claim:{...claim,source_url:"https://other.example.org/watch?v=abcdefghijk"}}]) {
      vi.mocked(invoke).mockResolvedValueOnce({...packet,web_source:{...receipt,...changed}});
      await expect(advanceDiscoveryCampaign(approved.id,approved.revision)).rejects.toThrow();
    }
    vi.mocked(invoke).mockResolvedValueOnce({...packet,web_source:{...receipt,status:"failed",native_author_id:null,candidate_id:null,is_primary_source:false,native_material:null,stop_code:"opencli_invalid_output"}});
    expect(initialMatchHit(await advanceDiscoveryCampaign(approved.id,approved.revision))).toBeNull();
  });

  it("continues native source author reads and scores only a new completed author", async () => {
    const claim = { campaign_id: approved.id, expected_revision: approved.revision, search_hit_id: "hit", platform: "douyin",
      profile_ref: "work-profile", source_url: "https://www.douyin.com/video/123", post_id: "123", call_number: 1 };
    const receipt = { claim, status: "completed", author_id: "MS4wLjABAAAA12345678", profile_url: "https://www.douyin.com/user/MS4wLjABAAAA12345678",
      duplicate_of: null, stop_code: null, captured_at: 100 };
    const step = { kind: "read_source_author", search_hit_id: "hit" };
    const packet = { ...response("read_source_author", 1), step, source_author: receipt,
      campaign: { ...approved, used_calls: 1, scopes: [{ platform: "douyin", profile_ref: "work-profile", keywords: ["keyboard review"] }], platform_runs: [] } };
    vi.mocked(invoke).mockResolvedValueOnce(packet).mockResolvedValueOnce({ ...packet, step: { kind: "needs_evidence" }, source_author: undefined });
    const onStep = vi.fn();
    expect(await runDiscoveryCampaign(approved, new AbortController().signal, onStep)).toBe("needs_evidence");
    expect(initialMatchHit(onStep.mock.calls[0][0])).toBe("hit");
    for (const changed of [{ status: "failed", author_id: null, profile_url: null, stop_code: "opencli_invalid_output" },
      { duplicate_of: "first-hit" }]) {
      vi.mocked(invoke).mockResolvedValueOnce({ ...packet, source_author: { ...receipt, ...changed } });
      expect(initialMatchHit(await advanceDiscoveryCampaign(approved.id, approved.revision))).toBeNull();
    }
    for (const changed of [{ claim: { ...claim, source_url: "https://www.douyin.com/video/456" } },
      { claim: { ...claim, profile_ref: "other" } }, { claim: { ...claim, expected_revision: approved.revision + 1 } },
      { claim: { ...claim, search_hit_id: "other" } }, { claim: { ...claim, call_number: 2 } },
      { profile_url: "https://www.douyin.com/user/other" }, { author_id: null }]) {
      vi.mocked(invoke).mockResolvedValueOnce({ ...packet, source_author: { ...receipt, ...changed } });
      await expect(advanceDiscoveryCampaign(approved.id, approved.revision)).rejects.toThrow();
    }
  });

  it("waits for saved-evidence scoring and never starts another read after pause", async () => {
    let finish!: (value: null) => void;
    let notify!: () => void;
    const started = new Promise<void>(resolve => { notify = resolve; });
    const model = new Promise<null>(resolve => { finish = resolve; });
    vi.mocked(invoke).mockImplementation(async command => {
      if (command === "onecrew_discovery_match_evaluate") { notify(); return model; }
      return { ...response("read_video", 1), step: { kind: "read_video", profile_ref: "work-profile", search_hit_id: "hit" } };
    });
    const controller = new AbortController();
    const run = runDiscoveryCampaign(approved, controller.signal, async result => {
      const hit = initialMatchHit(result);
      if (hit) await evaluateDiscoveryMatch({ campaign_id: result.campaign.id,
        expected_revision: result.campaign.revision, search_hit_id: hit });
    });
    await started;
    expect(invoke).toHaveBeenCalledTimes(2);
    controller.abort();
    finish(null);
    expect(await run).toBe("paused");
    expect(invoke).toHaveBeenCalledTimes(2);
  });
  it("continues the runner after a completed read whose model scoring failed before submission", async () => {
    const deferred = vi.fn();
    vi.mocked(invoke)
      .mockResolvedValueOnce({ ...response("read_video", 1), step: { kind: "read_video", profile_ref: "work-profile", search_hit_id: "hit" } })
      .mockRejectedValueOnce("initial_match_discovery_draft_plan_invalid")
      .mockResolvedValueOnce(response("needs_evidence", 1));
    const visited: string[] = [];
    expect(await runDiscoveryCampaign(approved, new AbortController().signal, async result => {
      visited.push(result.step.kind);
      const hit = initialMatchHit(result);
      if (hit) await evaluateDiscoveryMatch({ campaign_id: result.campaign.id,
        expected_revision: result.campaign.revision, search_hit_id: hit }, false, deferred);
    })).toBe("needs_evidence");
    expect(visited).toEqual(["read_video", "needs_evidence"]);
    expect(deferred).toHaveBeenCalledExactlyOnceWith("initial_match_discovery_draft_plan_invalid");
    expect(vi.mocked(invoke).mock.calls.map(call => call[0])).toEqual([
      "onecrew_discovery_advance", "onecrew_discovery_match_evaluate", "onecrew_discovery_advance",
    ]);
  });

  it("reads a stored unavailable transcript without dispatching a provider read", async () => {
    const receipt = { search_hit_id:"hit",candidate_id:"candidate",source_url:"https://www.youtube.com/watch?v=LR04bU_yV5k",
      profile_ref:"work-profile",platform_account_id:"UC1234567890",status:"unavailable",segments:[],truncated:false,stop_code:null,call_number:3,captured_at:100 };
    vi.mocked(invoke).mockResolvedValueOnce(receipt);
    expect(await getDiscoveryTranscript("campaign-1","hit")).toEqual(receipt);
    expect(invoke).toHaveBeenCalledExactlyOnceWith("onecrew_discovery_transcript_get",{input:{campaign_id:"campaign-1",search_hit_id:"hit"}});
  });

  it("follows server steps sequentially and does not count pending people as target reached", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(response("search", 1)).mockResolvedValueOnce(response("needs_evidence", 1));
    const onStep = vi.fn();
    expect(await runDiscoveryCampaign(approved, new AbortController().signal, onStep)).toBe("needs_evidence");
    expect(onStep).toHaveBeenCalledTimes(2);
    expect(vi.mocked(invoke).mock.calls).toEqual(Array(2).fill([
      "onecrew_discovery_advance", { input: { campaign_id: approved.id, expected_revision: approved.revision, excluded_platforms: [] } },
    ]));
  });

  it("stops the next read but accepts the current checkpoint after cancellation", async () => {
    const controller = new AbortController();
    vi.mocked(invoke).mockImplementationOnce(async () => { controller.abort(); return response("search", 1); });
    const onStep = vi.fn();
    expect(await runDiscoveryCampaign(approved, controller.signal, onStep)).toBe("paused");
    expect(onStep).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("continues after a budgeted transcript and keeps its source material separate from qualification", async () => {
    const material = {
      search_hit_id: "hit-1", candidate_id: "candidate-1", source_url: "https://www.youtube.com/watch?v=LR04bU_yV5k",
      profile_ref: "work-profile", platform_account_id: "UC1234567890", status: "completed",
      segments: [{ timestamp: "0:10", speaker: "", text: "Here I open the tool." }],
      truncated: true, stop_code: null, call_number: 1, captured_at: 100,
    };
    vi.mocked(invoke).mockResolvedValueOnce({ ...response("read_transcript", 1),
      step: { kind: "read_transcript", profile_ref: "work-profile", search_hit_id: "hit-1" }, transcript: material,
    }).mockResolvedValueOnce(response("needs_evidence", 1));
    const onStep = vi.fn();
    expect(await runDiscoveryCampaign(approved, new AbortController().signal, onStep)).toBe("needs_evidence");
    expect(onStep.mock.calls[0][0].transcript).toEqual(material);
    expect(onStep.mock.calls[0][0].counts.qualified).toBe(0);
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it("never dispatches from an already stopped window", async () => {
    const controller = new AbortController(); controller.abort();
    expect(await runDiscoveryCampaign(approved, controller.signal, vi.fn())).toBe("paused");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("continues a source-bound public page without treating it as qualification", async () => {
    const page = { candidate_id: "candidate-1", requested_url: "https://example.com/tool", search_hit_id: "hit-1",
      source_url: "https://www.youtube.com/watch?v=LR04bU_yV5k", source_excerpt: "Use https://example.com/tool",
      profile_ref: "work-profile", platform_account_id: "UC1234567890", call_number: 1,
      status: "completed", document_url: "https://example.com/tool", title: "Tool", markdown_excerpt: "Observed page",
      truncated: false, stop_code: null, captured_at: 100 };
    const step = { kind: "read_public_page", profile_ref: "work-profile", candidate_id: "candidate-1", requested_url: page.requested_url };
    vi.mocked(invoke).mockResolvedValueOnce({ ...response("read_public_page", 1), step, public_page: page })
      .mockResolvedValueOnce(response("needs_evidence", 1));
    const onStep = vi.fn();
    expect(await runDiscoveryCampaign(approved, new AbortController().signal, onStep)).toBe("needs_evidence");
    expect(onStep.mock.calls[0][0].public_page).toEqual(page);
    expect(onStep.mock.calls[0][0].counts.qualified).toBe(0);
    for (const patch of [{ candidate_id: "wrong" }, { profile_ref: "other" }, { requested_url: "https://example.com/other" },
      { call_number: 2 }, { status: "failed" }, { document_url: null }]) {
      vi.mocked(invoke).mockResolvedValueOnce({ ...response("read_public_page", 1), step, public_page: { ...page, ...patch } });
      await expect(runDiscoveryCampaign(approved, new AbortController().signal, vi.fn())).rejects.toThrow();
    }
    vi.mocked(invoke).mockResolvedValueOnce({ ...response("read_public_page", 1), step });
    await expect(runDiscoveryCampaign(approved, new AbortController().signal, vi.fn())).rejects.toThrow();
  });

  it("continues a committed contact lookup without promoting search leads to qualified authors", async () => {
    const planned={campaign_id:approved.id,expected_revision:approved.revision,candidate_id:"candidate-1",profile_ref:"work-profile",
      native_author_id:"author",author_label:"Creator",username:null,method:"search",query:'"Creator" email contact Telegram',
      call_number:0,status:"planned",links:[],stop_code:null};
    const step={kind:"read_contact_lookup",lookup:planned};
    const receipt={...planned,call_number:1,status:"completed",links:["https://creator.example/contact"]};
    vi.mocked(invoke).mockResolvedValueOnce({...response("read_contact_lookup",1),step,contact_lookup:receipt})
      .mockResolvedValueOnce(response("needs_evidence",1));
    const onStep=vi.fn();
    expect(await runDiscoveryCampaign(approved,new AbortController().signal,onStep)).toBe("needs_evidence");
    expect(onStep.mock.calls[0][0].contact_lookup).toEqual(receipt);
    expect(onStep.mock.calls[0][0].counts.qualified).toBe(0);
    for(const patch of [{candidate_id:"other"},{native_author_id:"other"},{call_number:2},{status:"reading"}]) {
      vi.mocked(invoke).mockResolvedValueOnce({...response("read_contact_lookup",1),step,contact_lookup:{...receipt,...patch}});
      await expect(advanceDiscoveryCampaign(approved.id,approved.revision)).rejects.toThrow();
    }
  });

  it("continues a budgeted author read and recovers its receipt with a local-only call", async () => {
    const profile = { claim: {campaign_id:"campaign-1",expected_revision:2,search_hit_id:"hit-profile",
      platform:"bilibili",profile_ref:"work-profile",author_id:"123",author_url:"https://space.bilibili.com/123",
      source_url:"https://www.bilibili.com/video/BV1RYqHYnEw8",call_number:1},
      status:"completed",native_profile:{mid:"123",follower_count:15000},stop_code:null,captured_at:100 };
    vi.mocked(invoke).mockResolvedValueOnce({...response("read_profile",1),step:{kind:"read_profile",search_hit_id:"hit-profile"},profile})
      .mockResolvedValueOnce(response("needs_evidence",1)).mockResolvedValueOnce(profile);
    const onStep = vi.fn();
    expect(await runDiscoveryCampaign(approved,new AbortController().signal,onStep)).toBe("needs_evidence");
    expect(onStep.mock.calls[0][0].profile).toEqual(profile);
    expect(onStep.mock.calls[0][0].counts.qualified).toBe(0);
    expect(await getDiscoveryProfile("campaign-1","hit-profile")).toEqual(profile);
    expect(vi.mocked(invoke).mock.calls[2]).toEqual(["onecrew_discovery_profile_get",{input:{campaign_id:"campaign-1",search_hit_id:"hit-profile"}}]);
    vi.mocked(invoke).mockResolvedValueOnce({...profile,native_profile:null});
    await expect(getDiscoveryProfile("campaign-1","hit-profile")).rejects.toThrow();
  });

  it("reads an author-bound video receipt locally and keeps qualification pending", async () => {
    const content = { claim: { campaign_id: "campaign-1", expected_revision: 2, search_hit_id: "hit-content",
      platform: "bilibili", profile_ref: "work-profile", author_id: "123",
      source_url: "https://www.bilibili.com/video/BV1RYqHYnEw8", call_number: 1 },
      status: "completed", native_content: { bvid: "BV1RYqHYnEw8", author_id: "123", author: "Creator",
        source_url: "https://www.bilibili.com/video/BV1RYqHYnEw8", title: "Product demo",
        description: "Public description", description_truncated: false, requires_payment: false, pay_preview: false },
      stop_code: null, captured_at: 100 };
    vi.mocked(invoke).mockResolvedValueOnce({ ...response("read_content", 1),
      step: { kind: "read_content", search_hit_id: "hit-content" }, content,
    }).mockResolvedValueOnce(response("needs_evidence", 1)).mockResolvedValueOnce(content);
    const onStep = vi.fn();
    expect(await runDiscoveryCampaign(approved, new AbortController().signal, onStep)).toBe("needs_evidence");
    expect(onStep.mock.calls[0][0].content).toEqual(content);
    expect(onStep.mock.calls[0][0].counts.qualified).toBe(0);
    expect(await getDiscoveryContent("campaign-1", "hit-content")).toEqual(content);
    expect(vi.mocked(invoke).mock.calls[2]).toEqual(["onecrew_discovery_content_get",
      { input: { campaign_id: "campaign-1", search_hit_id: "hit-content" } }]);
  });

  it("continues after saved Facebook target discovery without evaluating the locator as content", async () => {
    const content_targets = { claim: { campaign_id: "campaign-1", expected_revision: 2, search_hit_id: "fb-hit",
      platform: "facebook", profile_ref: "work-profile", author_id: "123", author_url: "https://www.facebook.com/profile.php?id=123",
      source_url: "https://www.facebook.com/profile.php?id=123", call_number: 1 }, status: "completed",
      source_urls: ["https://www.facebook.com/reel/451"], stop_code: null, captured_at: 100 };
    vi.mocked(invoke).mockResolvedValueOnce({ ...response("read_content_targets", 1), step: { kind: "read_content_targets", search_hit_id: "fb-hit" }, content_targets })
      .mockResolvedValueOnce(response("needs_evidence", 1));
    const onStep = vi.fn();
    expect(await runDiscoveryCampaign(approved, new AbortController().signal, onStep)).toBe("needs_evidence");
    expect(onStep).toHaveBeenCalledTimes(2);
    expect(initialMatchHit(onStep.mock.calls[0][0])).toBeNull();
  });

  it.each(["twitter", "youtube", "facebook"])("accepts a bound %s language supplement and rejects missing or mismatched evidence", async platform => {
    const source = platform === "facebook" ? "https://www.facebook.com/reel/451"
      : platform === "youtube" ? "https://www.youtube.com/watch?v=abcdefghijk" : "https://x.com/creator/status/123";
    const read = { claim: { campaign_id: "campaign-1", expected_revision: 2, search_hit_id: "language-hit",
      platform, profile_ref: "work-profile", author_id: "123", source_url: source, call_number: 1 }, status: "completed",
      native_content: null, language_sample: { author_id: "123", source_url: source, text: "Original text",
        comments: [{ source_url: source, text: "Me gusta este producto" }] }, stop_code: null, captured_at: 100 };
    vi.mocked(invoke).mockResolvedValueOnce({ ...response("read_content", 1), step: { kind: "read_content", search_hit_id: "language-hit" }, content: read })
      .mockResolvedValueOnce(response("needs_evidence", 1));
    expect(await runDiscoveryCampaign(approved, new AbortController().signal, vi.fn())).toBe("needs_evidence");
    for (const language_sample of [undefined, { ...read.language_sample, author_id: "999" }, { ...read.language_sample, source_url: "https://example.com/other" }]) {
      vi.mocked(invoke).mockResolvedValueOnce({ ...read, language_sample });
      await expect(getDiscoveryContent("campaign-1", "language-hit")).rejects.toThrow();
    }
  });

  it("accepts source-bound Douyin language fallback with the original first five positions", async () => {
    const source = "https://www.douyin.com/video/7688639864627214835";
    const author = "MS4wLjABAAAA12345678";
    const read = { claim: { campaign_id: "campaign-1", expected_revision: 2, search_hit_id: "language-hit",
      platform: "douyin", profile_ref: "work-profile", author_id: author, source_url: source, call_number: 1 },
      status: "completed", native_content: null, language_sample: { author_id: author, source_url: source,
        text: "a".repeat(9000), comments: Array.from({ length: 5 }, (_, index) => ({
          source_url: `${source}#comment-${index + 1}`, text: index === 0 ? "" : "これは便利な製品ですね",
        })) }, stop_code: null, captured_at: 100 };
    vi.mocked(invoke).mockResolvedValueOnce({ ...response("read_content", 1),
      step: { kind: "read_content", search_hit_id: "language-hit" }, content: read })
      .mockResolvedValueOnce(response("needs_evidence", 1));
    const onStep = vi.fn();
    expect(await runDiscoveryCampaign(approved, new AbortController().signal, onStep)).toBe("needs_evidence");
    expect(onStep.mock.calls[0][0].content).toEqual(read);
    expect(onStep.mock.calls[0][0].counts.qualified).toBe(0);
    const sample = read.language_sample;
    for (const language_sample of [
      { ...sample, author_id: "MS4wLjABAAAA87654321" },
      { ...sample, text: "日".repeat(4001) },
      { ...sample, text: "bad\u0000text" },
      { ...sample, comments: [...sample.comments, { source_url: `${source}#comment-6`, text: "第六条" }] },
      { ...sample, comments: [sample.comments[1], sample.comments[1]] },
      { ...sample, comments: [{ source_url: "https://www.douyin.com/video/9#comment-1", text: "別の動画" }] },
      { ...sample, comments: [{ source_url: `${source}#comment-invalid`, text: "不明な識別子" }] },
      { ...sample, comments: [{ source_url: `${source}#comment-1`, text: "日".repeat(667) }] },
    ]) {
      vi.mocked(invoke).mockResolvedValueOnce({ ...read, language_sample });
      await expect(getDiscoveryContent("campaign-1", "language-hit")).rejects.toThrow();
    }
    const failed = { ...read, status: "failed", language_sample: undefined, stop_code: "invalid_output" };
    vi.mocked(invoke).mockResolvedValueOnce(failed);
    expect(await getDiscoveryContent("campaign-1", "language-hit")).toEqual(failed);
  });

  it("accepts a saved X recent-content receipt without inferring qualification", async () => {
    const content = { claim: { campaign_id: "campaign-1", expected_revision: 2, search_hit_id: "x-hit",
      platform: "twitter", profile_ref: "work-profile", author_id: "creator_1",
      source_url: "https://x.com/creator_1/status/123456789", call_number: 3 },
      status: "completed", native_content: { author_id: "creator_1", profile_url: "https://x.com/creator_1",
        sampled_posts: 2, excluded_retweets: 1, identity_kind: "mutable_handle",
        original_posts: [{ id: "123456789", source_url: "https://x.com/creator_1/status/123456789",
          text: "Demo text is only a clue", text_truncated: false, created_at: "Fri Sep 25 23:00:10 +0000 2026",
          has_media: true, quotes_another_post: false }] }, stop_code: null, captured_at: 100 };
    vi.mocked(invoke).mockResolvedValueOnce(content);
    expect(await getDiscoveryContent("campaign-1", "x-hit")).toEqual(content);
  });

  it("preserves pending when distinct original sources are exhausted without evaluating failed content", async () => {
    const content = { claim: { campaign_id: "campaign-1", expected_revision: 2, search_hit_id: "x-hit",
      platform: "twitter", profile_ref: "work-profile", author_id: "123",
      source_url: "https://x.com/creator/status/456", call_number: 1 }, status: "failed",
      native_content: null, stop_code: "language_sources_exhausted", captured_at: 100 };
    vi.mocked(invoke).mockResolvedValueOnce({ ...response("read_content", 1),
      step: { kind: "read_content", search_hit_id: "x-hit" }, content })
      .mockResolvedValueOnce(response("needs_evidence", 1));
    const onStep = vi.fn();
    expect(await runDiscoveryCampaign(approved, new AbortController().signal, onStep)).toBe("needs_evidence");
    expect(onStep).toHaveBeenCalledTimes(2);
    expect(initialMatchHit(onStep.mock.calls[0][0])).toBeNull();
  });

  it("accepts native focal content but rejects unrelated author, post and Campaign responses", async () => {
    const content = { claim: { campaign_id: "campaign-1", expected_revision: 2, search_hit_id: "x-hit",
      platform: "twitter", profile_ref: "work-profile", author_id: "creator_1",
      source_url: "https://x.com/i/status/123", call_number: 4 },
      status: "completed", native_content: { adapter_version: "1.0.0", identity: {
        adapter_version: "1.0.0", account_id: "987654321", post_id: "123", handle: "creator_1",
        profile_url: "https://x.com/creator_1", source_url: "https://x.com/creator_1/status/123",
        identity_kind: "native_post_author_id", evidence_source: "x_tweet_detail",
      }, text: "Only a public observation", text_truncated: false, text_source: "note_tweet",
      created_at: "Fri Sep 25 23:00:10 +0000 2026", has_media: true, quotes_another_post: false,
      expanded_urls: ["https://example.com/tool"] }, stop_code: null, captured_at: 100 };
    vi.mocked(invoke).mockResolvedValueOnce(content);
    expect(await getDiscoveryContent("campaign-1", "x-hit")).toEqual(content);
    for (const identity of [
      { ...content.native_content.identity, handle: "other" },
      { ...content.native_content.identity, post_id: "999" },
      { ...content.native_content.identity, source_url: "https://evil.example/" },
      { ...content.native_content.identity, account_id: 987654321 },
    ]) {
      vi.mocked(invoke).mockResolvedValueOnce({ ...content, native_content: { ...content.native_content, identity } });
      await expect(getDiscoveryContent("campaign-1", "x-hit")).rejects.toThrow();
    }
    vi.mocked(invoke).mockResolvedValueOnce(content);
    await expect(getDiscoveryContent("other", "x-hit")).rejects.toThrow("discovery_content_scope_changed");
    vi.mocked(invoke).mockResolvedValueOnce(content);
    await expect(getDiscoveryContent("campaign-1", "other")).rejects.toThrow("discovery_content_scope_changed");
  });

  it("continues after committed material provider stops without inventing successful material", async () => {
    const stopped = { ...response("read_identity", 1), step: { kind: "read_identity", search_hit_id: "hit" },
      provider_stop_code: "opencli_login_required",
      campaign: { ...approved, used_calls: 1, platform_runs: [{ platform: "twitter", profile_ref: "work-profile", status: "unauthorized", calls_used: 1, stop_code: "opencli_login_required" }] } };
    for (const step of [stopped.step, { kind: "read_content", search_hit_id: "hit" },
      { kind: "read_public_page", candidate_id: "candidate", profile_ref: "work-profile", requested_url: "https://example.com/" }]) {
      vi.mocked(invoke).mockReset().mockResolvedValueOnce({ ...stopped, step }).mockResolvedValueOnce(response("needs_evidence", 1));
      expect(await runDiscoveryCampaign(approved, new AbortController().signal, vi.fn())).toBe("needs_evidence");
      expect(invoke).toHaveBeenCalledTimes(2);
      vi.mocked(invoke).mockResolvedValueOnce({ ...stopped, step, campaign: { ...approved, used_calls: 1 } });
      await expect(runDiscoveryCampaign(approved, new AbortController().signal, vi.fn())).rejects.toThrow();
    }
  });

  it("rechecks excluded sources before every step instead of retaining the starting login list", async () => {
    const exclusions = vi.fn().mockReturnValueOnce([]).mockReturnValueOnce(["twitter"]);
    vi.mocked(invoke).mockResolvedValueOnce(response("search",1)).mockResolvedValueOnce(response("needs_evidence",1));
    expect(await runDiscoveryCampaign(approved,new AbortController().signal,vi.fn(),exclusions)).toBe("needs_evidence");
    expect(vi.mocked(invoke).mock.calls[1][1]).toEqual({input:{campaign_id:approved.id,expected_revision:approved.revision,excluded_platforms:["twitter"]}});
  });

  it("does not retry an uncertain response or a non-progressing server result", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("connection lost"));
    await expect(runDiscoveryCampaign(approved, new AbortController().signal, vi.fn())).rejects.toThrow("connection lost");
    expect(invoke).toHaveBeenCalledTimes(1);
    vi.mocked(invoke).mockReset().mockResolvedValue(response("search", 0));
    await expect(runDiscoveryCampaign(approved, new AbortController().signal, vi.fn())).rejects.toThrow("discovery_progress_unconfirmed");
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("rejects unknown steps rather than treating them as completed", async () => {
    vi.mocked(invoke).mockResolvedValue(response("send_messages", 1));
    await expect(runDiscoveryCampaign(approved, new AbortController().signal, vi.fn())).rejects.toThrow();
    expect(invoke).toHaveBeenCalledTimes(1);
  });
});

describe("YouTube Campaign search gate", () => {
  it("requires an approved plan and its exact browser identity", () => {
    expect(canSearchDiscovery(approved, "work-profile")).toBe(true);
    expect(canSearchDiscovery(approved, "other-profile")).toBe(false);
    expect(canSearchDiscovery({ ...approved, phase: "draft" }, "work-profile")).toBe(false);
  });

  it("retains read statistics without limiting reads and blocks a read already in flight", () => {
    expect(canSearchDiscovery({ ...approved, used_calls: 50 }, "work-profile")).toBe(true);
    expect(canSearchDiscovery({ ...approved, platform_runs: [{ ...approved.platform_runs[0], status: "searching" }] }, "work-profile")).toBe(false);
    expect(canSearchDiscovery({ ...approved, platform_runs: [{ ...approved.platform_runs[0], status: "technical_stop", stop_code: "opencli_timeout" }] }, "work-profile")).toBe(false);
    expect(canSearchDiscovery({ ...approved, platform_runs: [{ ...approved.platform_runs[0], status: "technical_stop", stop_code: "interrupted_read" }] }, "work-profile")).toBe(true);
    expect(canSearchDiscovery({ ...approved, platform_runs: [{ ...approved.platform_runs[0], status: "completed", calls_used: 1 }] }, "work-profile")).toBe(false);
  });
});

describe("multi-platform Campaign search gate", () => {
  it("keeps platform and profile frozen and checks each first-round run", () => {
    const multi: DiscoveryCampaign = {
      ...approved,
      scopes: [...approved.scopes, { platform: "bilibili", profile_ref: "work-profile", keywords: ["AI 工具"] }],
      platform_runs: [...approved.platform_runs, { platform: "bilibili", profile_ref: "work-profile", status: "unsearched", calls_used: 0, stop_code: null }],
    };
    expect(canSearchDiscovery(multi, "work-profile", "bilibili")).toBe(true);
    expect(canSearchDiscovery(multi, "other-profile", "bilibili")).toBe(false);
    const searchedYoutube = { ...multi, platform_runs: [{ ...multi.platform_runs[0], status: "completed", calls_used: 1 }, multi.platform_runs[1]] };
    expect(canSearchDiscovery(searchedYoutube, "work-profile", "youtube")).toBe(false);
    expect(canSearchDiscovery(searchedYoutube, "work-profile", "bilibili")).toBe(true);
  });
});

describe("explicit interrupted-search recovery", () => {
  const interrupted: DiscoveryCampaign = { ...approved, used_calls: 1,
    platform_runs: [{ ...approved.platform_runs[0], calls_used: 1, status: "technical_stop", stop_code: "interrupted_read" }] };

  it("requires an interrupted search, approved identity and finished coverage", () => {
    expect(canRecoverDiscoverySearch(interrupted, "work-profile", "youtube")).toBe(true);
    expect(canRecoverDiscoverySearch(interrupted, "other-profile", "youtube")).toBe(false);
    expect(canRecoverDiscoverySearch({ ...interrupted, phase: "draft" }, "work-profile", "youtube")).toBe(false);
    expect(canRecoverDiscoverySearch({ ...interrupted, used_calls: 50 }, "work-profile", "youtube")).toBe(true);
    for (const code of ["opencli_timeout", "opencli_command_failed", "opencli_auth_required", "interrupted_video_read"]) {
      expect(canRecoverDiscoverySearch({ ...interrupted, platform_runs: [{ ...interrupted.platform_runs[0], stop_code: code }] }, "work-profile", "youtube")).toBe(false);
    }
    for (const status of ["unsearched", "searching"]) {
      expect(canRecoverDiscoverySearch({ ...interrupted, platform_runs: [...interrupted.platform_runs,
        { platform: "twitter", profile_ref: "work-profile", status, calls_used: 0, stop_code: null }] }, "work-profile", "youtube")).toBe(false);
    }
  });

  it("uses one owner call without supplying a new query and rejects mismatched receipts", async () => {
    vi.mocked(invoke).mockReset().mockResolvedValue({ campaign: interrupted, returned_hits: 0 });
    expect(await searchDiscoveryOnce(interrupted.id, interrupted.revision, "youtube", "work-profile")).toEqual(interrupted);
    expect(invoke).toHaveBeenCalledExactlyOnceWith("onecrew_discovery_search_once", {
      id: interrupted.id, expectedRevision: interrupted.revision, platform: "youtube", profileRef: "work-profile", limit: 10,
    });
    for (const patch of [{ id: "other" }, { revision: 999 }]) {
      vi.mocked(invoke).mockResolvedValueOnce({ campaign: { ...interrupted, ...patch }, returned_hits: 0 });
      await expect(searchDiscoveryOnce(interrupted.id, interrupted.revision, "youtube", "work-profile")).rejects.toThrow("discovery_revision_changed");
    }
    vi.mocked(invoke).mockReset().mockRejectedValue(new Error("connection lost"));
    await expect(searchDiscoveryOnce(interrupted.id, interrupted.revision, "youtube", "work-profile")).rejects.toThrow("connection lost");
    expect(invoke).toHaveBeenCalledTimes(1);
  });
});

describe("YouTube video evidence gate", () => {
  it("waits for first search and exact profile regardless of accumulated read count", () => {
    expect(canVerifyDiscovery(approved, "work-profile")).toBe(false);
    expect(canVerifyDiscovery({ ...approved, platform_runs: [{ ...approved.platform_runs[0], status: "completed" }] }, "work-profile")).toBe(true);
    expect(canVerifyDiscovery({ ...approved, platform_runs: [{ ...approved.platform_runs[0], status: "completed" }] }, "other-profile")).toBe(false);
    expect(canVerifyDiscovery({ ...approved, used_calls: 50, platform_runs: [{ ...approved.platform_runs[0], status: "completed" }] }, "work-profile")).toBe(true);
  });
});
