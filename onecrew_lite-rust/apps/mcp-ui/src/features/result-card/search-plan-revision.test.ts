import { describe, expect, it } from "vitest";
import { reviseSearchPlanWithReadback } from "@mcp-ui/features/result-card/search-plan-revision";
import { reviseSearchPlan, confirmResult } from "@mcp-ui/lib/result-client";
import { appMock } from "@mcp-ui/test-support/app-mock";
import { SEARCH_PLAN, searchPlanSnapshot } from "@mcp-ui/test-support/search-plan-fixture";

describe("search plan typed revision", () => {
  it("uses the canonical new snapshot binding for subsequent confirmation", async () => {
    const original = searchPlanSnapshot();
    const current = searchPlanSnapshot({ ...SEARCH_PLAN, description: "Server canonical criteria" }, 3);
    const { app, callServerTool } = appMock([
      { content: [], structuredContent: { kind: "snapshot", snapshot: current } },
      { content: [], structuredContent: { kind: "snapshot", snapshot: { ...current, pending_action: null, state: { kind: "resolved", code: "search_plan_confirmed", summary: "Plan confirmed" } } } },
    ]);
    const result = await reviseSearchPlanWithReadback(app, original.binding, { description: "Edited criteria" });
    expect(result.kind).toBe("updated");
    if (result.kind !== "updated") throw new Error("Expected canonical preview");
    expect(result.snapshot.pending_action).toMatchObject({ draft: { description: "Server canonical criteria" } });
    await confirmResult(app, result.snapshot.binding);
    expect(callServerTool.mock.calls.map(([request]) => request.arguments)).toEqual([
      { action: "revise_search_plan", binding: original.binding, patch: { description: "Edited criteria" } },
      { action: "confirm", binding: current.binding },
    ]);
  });

  it("rejects a stale success response instead of enabling confirmation of old values", async () => {
    const original = searchPlanSnapshot();
    const { app } = appMock([{ content: [], structuredContent: { kind: "snapshot", snapshot: original } }]);
    await expect(reviseSearchPlan(app, original.binding, { keywords: ["baking"] })).rejects.toMatchObject({ failure: { kind: "invalid_response", reason: "snapshot_consistency_rejected" } });
  });

  it("reads current authority on a revision conflict without retrying the patch", async () => {
    const original = searchPlanSnapshot();
    const current = searchPlanSnapshot({ ...SEARCH_PLAN, description: "Concurrent criteria" }, 4);
    const { app, callServerTool } = appMock([
      { content: [], isError: true, structuredContent: { code: "revision_conflict", current_revision: 4 } },
      { content: [], structuredContent: { kind: "snapshot", snapshot: current } },
    ]);
    expect(await reviseSearchPlanWithReadback(app, original.binding, { keywords: ["baking"] })).toEqual({ kind: "rejected", failure: { kind: "server", failure: { code: "revision_conflict", current_revision: 4 } }, current });
    expect(callServerTool.mock.calls.map(([request]) => request.arguments?.action)).toEqual(["revise_search_plan", "read_current"]);
  });

  it("retains a rejected draft with no verified authority when the plan expired", async () => {
    const { app, callServerTool } = appMock([
      { content: [], isError: true, structuredContent: { code: "expired" } },
      { content: [], isError: true, structuredContent: { code: "expired" } },
    ]);
    expect(await reviseSearchPlanWithReadback(app, searchPlanSnapshot().binding, { description: "My draft" })).toEqual({ kind: "rejected", failure: { kind: "server", failure: { code: "expired" } }, current: null });
    expect(callServerTool).toHaveBeenCalledTimes(2);
  });

  it("reads back an ambiguous transport outcome without resending the mutation", async () => {
    const current = searchPlanSnapshot({ ...SEARCH_PLAN, description: "A different server draft" }, 3);
    const { app, callServerTool } = appMock([{ content: [], structuredContent: { kind: "snapshot", snapshot: current } }]);
    callServerTool.mockRejectedValueOnce(new Error("Response lost"));
    expect(await reviseSearchPlanWithReadback(app, searchPlanSnapshot().binding, { description: "My requested draft" })).toEqual({ kind: "rejected", failure: { kind: "transport" }, current });
    expect(callServerTool.mock.calls.map(([request]) => request.arguments?.action)).toEqual(["revise_search_plan", "read_current"]);
  });

  it("recognizes an applied revision from readback after its response is lost", async () => {
    const current = searchPlanSnapshot({ ...SEARCH_PLAN, description: "My requested draft" }, 3);
    const { app, callServerTool } = appMock([{ content: [], structuredContent: { kind: "snapshot", snapshot: current } }]);
    callServerTool.mockRejectedValueOnce(new Error("Response lost after commit"));
    expect(await reviseSearchPlanWithReadback(app, searchPlanSnapshot().binding, { description: "My requested draft", keywords: ["cooking"] })).toEqual({ kind: "updated", snapshot: current });
    expect(callServerTool.mock.calls.map(([request]) => request.arguments?.action)).toEqual(["revise_search_plan", "read_current"]);
  });

  it("does not mistake a matching old revision or partially matching patch for success", async () => {
    for (const current of [searchPlanSnapshot(SEARCH_PLAN, 2), searchPlanSnapshot({ ...SEARCH_PLAN, description: "Different criteria" }, 3)]) {
      const { app, callServerTool } = appMock([{ content: [], structuredContent: { kind: "snapshot", snapshot: current } }]);
      callServerTool.mockRejectedValueOnce(new Error("Response lost"));
      expect(await reviseSearchPlanWithReadback(app, searchPlanSnapshot().binding, { description: SEARCH_PLAN.description, keywords: ["cooking"] })).toMatchObject({ kind: "rejected", current });
    }
  });
});
