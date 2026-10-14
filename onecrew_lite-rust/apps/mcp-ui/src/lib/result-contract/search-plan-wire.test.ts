import { describe, expect, it } from "vitest";
import { diagnoseResultSnapshot, parseResultSnapshot, parseResultStateOutput, resultSnapshotDiagnostics } from "@mcp-ui/lib/result-contract";
import { validateResultSnapshot } from "@mcp-ui/lib/generated/contracts/validators/ResultSnapshot.js";
import { validateResultStateOutput } from "@mcp-ui/lib/generated/contracts/validators/ResultStateOutput.js";
import { validateClaimedResultContinuation } from "@mcp-ui/lib/generated/contracts/validators/ClaimedResultContinuation.js";
import wire from "@mcp-ui/test-support/search-plan-wire.json";
import readCurrent from "@mcp-ui/test-support/search-plan-read-current.json";
import { readCurrentResult } from "@mcp-ui/lib/result-client";
import { appMock } from "@mcp-ui/test-support/app-mock";

function omitNull(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(omitNull);
  if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).filter(([, item]) => item !== null).map(([key, item]) => [key, omitNull(item)]));
  return value;
}

// Hand-built onecrew_result_present / read_current wires in the operation-bound
// scopes shape. They stand in for a live recording and must stay accepted by the
// regenerated validators without any App-side repair.
describe("hand-built SearchPlan scopes wire", () => {
  it("accepts the complete read_current tool response through the real client", async () => {
    const snapshot = parseResultSnapshot(wire);
    if (!snapshot) throw new Error("Expected hand-built snapshot");
    expect(validateResultStateOutput(readCurrent.structuredContent), JSON.stringify(validateResultStateOutput.errors)).toBe(true);
    expect(parseResultStateOutput(readCurrent.structuredContent)).not.toBeNull();
    expect(JSON.parse(readCurrent.content[0]!.text)).toEqual(readCurrent.structuredContent);
    const { app } = appMock([readCurrent]);
    expect(await readCurrentResult(app, snapshot.binding)).toEqual(parseResultSnapshot(readCurrent.structuredContent.snapshot));
  });

  it("accepts the onecrew_result_present snapshot without changing its strict schema", () => {
    expect(validateResultSnapshot(wire), JSON.stringify(validateResultSnapshot.errors)).toBe(true);
    expect(diagnoseResultSnapshot(wire)).toBeNull();
    expect(parseResultSnapshot(wire)).toEqual(wire);
    const output = { kind: "snapshot", snapshot: wire };
    expect(validateResultStateOutput(output), JSON.stringify(validateResultStateOutput.errors)).toBe(true);
    expect(parseResultStateOutput(output)).toEqual(output);
    expect(wire.pending_action.draft.scopes.map((scope) => scope.operation_id)).toEqual(["x.posts.search.advanced", "youtube.channels.search"]);
  });

  it("normalizes only the generic nullable envelope fields; scopes need no repair", () => {
    const omitted = omitNull(wire);
    expect(validateResultSnapshot(omitted)).toBe(false);
    expect(parseResultStateOutput({ kind: "snapshot", snapshot: omitted })).toEqual({ kind: "snapshot", snapshot: wire });
  });

  it("does not repair unknown keys, wrong types or absent required scope fields", () => {
    for (const mutate of [
      (scope: Record<string, unknown>) => { delete scope.operation_id; },
      (scope: Record<string, unknown>) => { delete scope.schema_revision; },
      (scope: Record<string, unknown>) => { delete scope.connection; },
      (scope: Record<string, unknown>) => { delete scope.arguments; },
      (scope: Record<string, unknown>) => { scope.arguments = []; },
      (scope: Record<string, unknown>) => { scope.unexpected = true; },
      (scope: Record<string, unknown>) => { scope.connection = { kind: "unknown_kind", toolkit: "x" }; },
      (scope: Record<string, unknown>) => { scope.connection = { kind: "operator_account", toolkit: "x" }; },
      (scope: Record<string, unknown>) => { scope.connection = { ...(scope.connection as object), extra: true }; },
      (scope: Record<string, unknown>) => { (scope.connection as Record<string, unknown>).toolkit = 12; },
      (scope: Record<string, unknown>) => { scope.operation_id = " "; },
      (scope: Record<string, unknown>) => { scope.schema_revision = ""; },
    ]) {
      const invalid = structuredClone(wire);
      mutate(invalid.pending_action.draft.scopes[0]!);
      expect(parseResultSnapshot(invalid)).toBeNull();
    }
  });

  it("rejects the retired draft vocabulary and an empty scope list", () => {
    const draft = wire.pending_action.draft;
    for (const retired of [
      { ...draft, searches: [] },
      { ...draft, tiktok_searches: [] },
      { description: draft.description, keywords: draft.keywords, searches: [], tiktok_searches: [] },
      { ...draft, scopes: [] },
      { ...draft, scopes: [{ ...draft.scopes[0]!, kind: "standard" }] },
    ]) expect(parseResultSnapshot({ ...wire, pending_action: { ...wire.pending_action, draft: retired } })).toBeNull();
  });

  it("reports schema rules and missing fields without including data or unknown field names", () => {
    const invalid = structuredClone(wire);
    const scope = invalid.pending_action.draft.scopes[0] as Record<string, unknown>;
    delete scope.operation_id;
    const missing = resultSnapshotDiagnostics(invalid);
    expect(missing.some((item) => item.keyword === "required" && item.missingProperty === "operation_id")).toBe(true);
    scope.operation_id = "x.posts.search.advanced";
    (scope.connection as Record<string, unknown>).toolkit = { privateValue: "do-not-output-this" };
    const diagnostics = JSON.stringify(resultSnapshotDiagnostics(invalid));
    expect(diagnostics).toContain("schemaPath");
    expect(diagnostics).not.toContain("do-not-output-this");
    expect(diagnostics).not.toContain("privateValue");
  });

  it("carries the confirmed criteria, not the scopes, through the claimed continuation contract", () => {
    const { description, keywords } = wire.pending_action.draft;
    const claimed = { binding: wire.binding, continuation_revision: 2, lease_id: `rcl_${"d".repeat(32)}`, request: { message: "Search plan confirmed. Continue with the approved searches.", requires_model_context: true, context: { kind: "search_plan_confirmed", confirmed_plan_ref: "12345678-1234-4234-8234-123456789abc", description, keywords } }, dispatch_started_at_ms: 10, dispatch_not_after_ms: 100 };
    expect(validateClaimedResultContinuation(claimed), JSON.stringify(validateClaimedResultContinuation.errors)).toBe(true);
    expect(parseResultStateOutput(omitNull({ kind: "claimed_continuation", claimed }))).toEqual({ kind: "claimed_continuation", claimed });
    const withScopes = { ...claimed, request: { ...claimed.request, context: { ...claimed.request.context, scopes: wire.pending_action.draft.scopes } } };
    expect(validateClaimedResultContinuation(withScopes)).toBe(false);
    const blankCriteria = { ...claimed, request: { ...claimed.request, context: { ...claimed.request.context, description: " " } } };
    expect(validateClaimedResultContinuation(blankCriteria)).toBe(true);
    expect(parseResultStateOutput({ kind: "claimed_continuation", claimed: blankCriteria })).toBeNull();
  });
});
