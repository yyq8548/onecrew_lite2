import { describe, expect, it } from "vitest";
import { canStartPreparedDraft, discoveryDraftFailureKey, matchesDiscoveryCodexRequest } from "./discovery-codex-request";
import type { DiscoveryCampaign } from "./opencli-discovery";

const request = { id: "draft-request", brief: "我要卖手机支架", profileRef: "browser", version: "1.8.8", platforms: ["youtube", "bilibili"], targetCount: 30 };
const draft: DiscoveryCampaign = { id: request.id, brief: request.brief, opencli_version: request.version,
  phase: "draft", revision: 1, criteria: {}, target_count: 30, used_calls: 0,
  scopes: [{ platform: "youtube", profile_ref: "browser", keywords: ["phone stand review"] }], platform_runs: [] };

describe("Automatic product planning", () => {
  it("accepts AI-understood creator count only when no manual override was supplied", () => {
    const understood = { ...request, brief: "在tiktok上搜索美妆博主5人", platforms: ["youtube", "tiktok"], targetCount: null };
    const planned = { ...draft, brief: understood.brief, target_count: 5,
      scopes: [{ platform: "tiktok", profile_ref: "browser", keywords: ["美妆"] }] };
    expect(matchesDiscoveryCodexRequest(planned, understood)).toBe(true);
    expect(matchesDiscoveryCodexRequest(planned, { ...understood, targetCount: 8 })).toBe(false);
    expect(matchesDiscoveryCodexRequest(planned, { ...understood, platforms: ["youtube"] })).toBe(false);
  });
  it("starts only the explicitly requested draft after selection, never a restored or changed plan", () => {
    const pending = { id: draft.id, profileRef: "browser" };
    expect(canStartPreparedDraft(pending, draft, draft.id, "browser")).toBe(true);
    expect(canStartPreparedDraft(null, draft, draft.id, "browser")).toBe(false);
    expect(canStartPreparedDraft(pending, draft, "old", "browser")).toBe(false);
    expect(canStartPreparedDraft(pending, draft, draft.id, "other")).toBe(false);
    for (const changed of [{ id: "other" }, { phase: "active" }, { used_calls: 1 }, { scopes: [] }, { scopes: [{ ...draft.scopes[0], profile_ref: "other" }] }]) {
      expect(canStartPreparedDraft(pending, { ...draft, ...changed }, draft.id, "browser")).toBe(false);
    }
  });
  it("rejects a response that changes an explicit follower or location setting", () => {
    const edited = { ...request, minFollowersOverride: 10000 };
    expect(matchesDiscoveryCodexRequest({ ...draft, criteria: { min_subscribers: 8000 } }, edited)).toBe(false);
    expect(matchesDiscoveryCodexRequest({ ...draft, criteria: { min_subscribers: 10000 } }, edited)).toBe(true);
    expect(matchesDiscoveryCodexRequest(draft, { ...request, criteriaOverrides: { region: "US", language: null, category: null, requires_ai_tool_demo: false, requires_public_tool_url: false } })).toBe(false);
  });
  it("shows actionable closed failures without echoing private causes", () => {
    expect(discoveryDraftFailureKey("discovery_draft_codex_login_required")).toBe("aiCodexLoginRequired");
    expect(discoveryDraftFailureKey("discovery_draft_outcome_unknown")).toBe("aiUncertain");
    expect(discoveryDraftFailureKey("discovery_draft_busy")).toBe("aiBusy");
    expect(discoveryDraftFailureKey("discovery_draft_input_invalid")).toBe("aiInvalidInput");
    expect(discoveryDraftFailureKey("discovery_draft_plan_invalid")).toBe("aiInvalidPlan");
    expect(discoveryDraftFailureKey("discovery_draft_intent_invalid")).toBe("aiInvalidIntent");
    expect(discoveryDraftFailureKey("discovery_draft_scope_invalid")).toBe("aiInvalidScope");
    expect(discoveryDraftFailureKey("discovery_draft_coverage_incomplete")).toBe("aiIncompleteCoverage");
    expect(discoveryDraftFailureKey(new Error("private-token-sentinel"))).toBe("aiFailed");
  });
  it("adopts only an unexecuted matching draft from the selected identity and available sources", () => {
    expect(matchesDiscoveryCodexRequest(draft, request)).toBe(true);
    for (const changed of [{ id: "other" }, { brief: "其他商品" }, { phase: "active" }, { used_calls: 1 }, { opencli_version: "2.0" },
      { target_count: 200 }, { scopes: [] }, { scopes: [{ ...draft.scopes[0], profile_ref: "other" }] }, { scopes: [{ ...draft.scopes[0], platform: "unknown" }] }]) {
      expect(matchesDiscoveryCodexRequest({ ...draft, ...changed }, request)).toBe(false);
    }
  });
  it("preserves supplied source terms when AI fills other fields", () => {
    expect(matchesDiscoveryCodexRequest(draft, { ...request, scopeOverrides: [{ platform: "youtube", keywords: ["manual"] }] })).toBe(false);
    expect(matchesDiscoveryCodexRequest(draft, { ...request, scopeOverrides: [{ platform: "youtube", keywords: ["phone stand review"] }] })).toBe(true);
  });
});

it("accepts language-normalized manual topics in an owner draft, preserving request identity", () => {
 const requested = {...request, criteriaOverrides: {region: null, language: "英文", category: null, requires_ai_tool_demo: false, requires_public_tool_url: false}, scopeOverrides: [{platform: "youtube", keywords: ["手机支架测评"]}]};
 const response = {...draft, criteria: {language: "英文", requires_ai_tool_demo: false, requires_public_tool_url: false}};
 expect(matchesDiscoveryCodexRequest(response, requested)).toBe(true);
 expect(matchesDiscoveryCodexRequest({...response, brief: "different"}, requested)).toBe(false);
});
