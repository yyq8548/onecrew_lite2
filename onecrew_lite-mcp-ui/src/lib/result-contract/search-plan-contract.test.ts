import { describe, expect, it } from "vitest";
import { parseResultSnapshot, parseResultStateOutput, parseResultStateFailure, searchPlanEditorDraftValid, searchPlanKeywordsValid, searchPlanScopesValid, SEARCH_PLAN_SCOPE_MAX_COUNT, type SearchPlanEditorDraft } from "@mcp-ui/lib/result-contract";
import { INSTAGRAM_SCOPE, SEARCH_PLAN, X_SCOPE, YOUTUBE_SCOPE, searchPlanSnapshot } from "@mcp-ui/test-support/search-plan-fixture";

const TAB = String.fromCharCode(9);
const NEXT_LINE = String.fromCharCode(0x85);

describe("search plan contract refinements", () => {
  it("accepts every tagged connection kind and open operation-owned arguments without a keyword requirement", () => {
    const draft: SearchPlanEditorDraft = { ...SEARCH_PLAN, keywords: [], scopes: [INSTAGRAM_SCOPE, YOUTUBE_SCOPE, X_SCOPE] };
    const snapshot = searchPlanSnapshot(draft);
    expect(parseResultSnapshot(snapshot)).toEqual(snapshot);
    const nested = searchPlanSnapshot({ ...SEARCH_PLAN, scopes: [{ ...YOUTUBE_SCOPE, arguments: { query: "cooking", filters: { region: null, tags: ["a", "b"], nested: { deep: true } }, count: 0 } }] });
    expect(parseResultSnapshot(nested)).toEqual(nested);
    const empty = searchPlanSnapshot({ ...SEARCH_PLAN, scopes: [{ ...YOUTUBE_SCOPE, arguments: {} }] });
    expect(parseResultSnapshot(empty)).toEqual(empty);
  });

  it("follows validate_typed for description, keywords and scope count", () => {
    expect(searchPlanEditorDraftValid(SEARCH_PLAN)).toBe(true);
    expect(searchPlanEditorDraftValid({ ...SEARCH_PLAN, description: "  " })).toBe(false);
    expect(searchPlanEditorDraftValid({ ...SEARCH_PLAN, description: "汉".repeat(2730) })).toBe(true);
    expect(searchPlanEditorDraftValid({ ...SEARCH_PLAN, description: "汉".repeat(2731) })).toBe(false);
    expect(searchPlanKeywordsValid([])).toBe(true);
    expect(searchPlanKeywordsValid(Array.from({ length: 50 }, (_, index) => `k${index}`))).toBe(true);
    expect(searchPlanKeywordsValid(Array.from({ length: 51 }, (_, index) => `k${index}`))).toBe(false);
    for (const invalid of [[""], [" cooking"], ["cooking "], [`x${TAB}y`], [`x${NEXT_LINE}y`], ["汉".repeat(171)], ["cooking", "cooking"]]) expect(searchPlanKeywordsValid(invalid)).toBe(false);
    expect(searchPlanKeywordsValid(["汉".repeat(170), "RWA México", "x".repeat(101)])).toBe(true);
    expect(searchPlanScopesValid([])).toBe(false);
    expect(searchPlanScopesValid(Array.from({ length: SEARCH_PLAN_SCOPE_MAX_COUNT }, () => INSTAGRAM_SCOPE))).toBe(true);
    expect(searchPlanScopesValid(Array.from({ length: SEARCH_PLAN_SCOPE_MAX_COUNT + 1 }, () => INSTAGRAM_SCOPE))).toBe(false);
    expect(searchPlanScopesValid([{ ...INSTAGRAM_SCOPE, operation_id: " " }])).toBe(false);
    expect(searchPlanScopesValid([{ ...INSTAGRAM_SCOPE, schema_revision: "" }])).toBe(false);
    for (const draft of [{ ...SEARCH_PLAN, scopes: [] }, { ...SEARCH_PLAN, keywords: ["cooking", "cooking"] }, { ...SEARCH_PLAN, description: "汉".repeat(3000) }]) {
      expect(parseResultSnapshot(searchPlanSnapshot(draft))).toBeNull();
    }
  });

  it("rejects a draft over the 512 KiB payload bound", () => {
    const oversized = searchPlanSnapshot({ ...SEARCH_PLAN, scopes: [{ ...YOUTUBE_SCOPE, arguments: { query: "x".repeat(600 * 1024) } }] });
    expect(parseResultSnapshot(oversized)).toBeNull();
  });

  it("parses the explicit plan capacity error without accepting unknown fields", () => {
    expect(parseResultStateFailure({ code: "search_plan_payload_too_large" })).toEqual({ code: "search_plan_payload_too_large" });
    expect(parseResultStateFailure({ code: "search_plan_payload_too_large", raw: "hidden" })).toBeNull();
  });

  it("accepts the exact available search plan and rejects mismatched authority kinds", () => {
    const snapshot = searchPlanSnapshot();
    expect(parseResultSnapshot(snapshot)).toEqual(snapshot);
    expect(parseResultSnapshot({ ...snapshot, presentation: { ...snapshot.presentation, kind: "simple_operation" } })).toBeNull();
    expect(parseResultSnapshot({ ...snapshot, pending_action: { kind: "confirm", prompt: "Confirm", label: "Confirm" } })).toBeNull();
    expect(parseResultSnapshot({ ...snapshot, pending_action: { ...snapshot.pending_action, draft_digest: "unverified" } })).toBeNull();
  });

  it("rejects unknown draft fields, retired search vocabulary and malformed scopes", () => {
    const snapshot = searchPlanSnapshot();
    for (const draft of [
      { ...SEARCH_PLAN, injected: true },
      { ...SEARCH_PLAN, searches: [] },
      { ...SEARCH_PLAN, tiktok_searches: [] },
      { ...SEARCH_PLAN, scopes: [{ ...INSTAGRAM_SCOPE, kind: "standard" }] },
      { ...SEARCH_PLAN, scopes: [{ operation_id: "incomplete" }] },
    ]) expect(parseResultSnapshot({ ...snapshot, pending_action: { ...snapshot.pending_action, draft } })).toBeNull();
  });

  it("carries the confirmed ref and criteria in continuation context and rejects a plan dump or invalid criteria", () => {
    const snapshot = searchPlanSnapshot();
    const context = { kind: "search_plan_confirmed", confirmed_plan_ref: "12345678-1234-4234-8234-123456789abc", description: SEARCH_PLAN.description, keywords: SEARCH_PLAN.keywords };
    const output = { kind: "claimed_continuation", claimed: {
      binding: snapshot.binding, continuation_revision: 2, lease_id: `rcl_${"d".repeat(32)}`,
      request: { message: "已确认搜索计划，请按计划继续搜索。", requires_model_context: true, context },
      dispatch_started_at_ms: 10, dispatch_not_after_ms: 100,
    } };
    expect(parseResultStateOutput(output)).toEqual(output);
    for (const invalid of [
      { ...context, plan: { ...SEARCH_PLAN, scopes: [INSTAGRAM_SCOPE, YOUTUBE_SCOPE, X_SCOPE] } },
      { ...context, scopes: [INSTAGRAM_SCOPE] },
      { ...context, description: "  " },
      { ...context, keywords: ["beauty", "beauty"] },
      { ...context, confirmed_plan_ref: "plan-ref" },
    ]) {
      expect(parseResultStateOutput({ ...output, claimed: { ...output.claimed, request: { ...output.claimed.request, context: invalid } } })).toBeNull();
    }
  });
});
