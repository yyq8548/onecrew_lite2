import { describe, expect, it, vi } from "vitest";

import {
  cancelResult,
  callResultState,
  classifyInvalidResultResponse,
  confirmResult,
  continuationRefreshDelay,
  dispatchPendingContinuation,
  resultRefreshDelay,
  SUBMITTED_REFRESH_ATTEMPTS,
  readCurrentResult,
  requestAgentEdit,
  RESULT_STATE_TOOL_NAME,
  resultStateTimeoutMs,
  selectResult,
} from "@mcp-ui/lib/result-client";
import type { ResultSnapshot } from "@mcp-ui/lib/result-contract";
import { appMock } from "@mcp-ui/test-support/app-mock";
import { BINDING, terminalSnapshot } from "@mcp-ui/test-support/fixtures";
import { DM_FAILURE, dmFailedSnapshot } from "@mcp-ui/test-support/dm-failure-fixture";

function submittedSnapshot(): ResultSnapshot {
  return {
    ...terminalSnapshot(),
    state: { kind: "submitted", action: { kind: "confirmed" } },
    continuation: { state: "not_requested", continuation_revision: 0, dispatch_started_at_ms: null, dispatch_not_after_ms: null },
  };
}

describe("MCP App V2 App-only actions", () => {
  it("pauses instead of sending an incomplete answer summary when full context delivery fails", async () => {
    const snapshot = terminalSnapshot();
    const claimed = {
      binding: snapshot.binding, continuation_revision: 3, lease_id: `rcl_${"d".repeat(32)}`,
      request: { message: "A bounded summary.", requires_model_context: true,
        context: { kind: "choice", answers: [{ question_id: "q", text: "The complete custom answer" }] } },
      dispatch_started_at_ms: 10, dispatch_not_after_ms: 130,
    };
    const completed = { binding: { ...snapshot.binding, revision: 4 }, continuation: {
      state: "paused", continuation_revision: 4, dispatch_started_at_ms: null, dispatch_not_after_ms: null,
    } };
    const { app, callServerTool, updateModelContext, sendMessage } = appMock([
      { content: [], structuredContent: { kind: "claimed_continuation", claimed } },
      { content: [], structuredContent: { kind: "completed_continuation", completed } },
    ]);
    updateModelContext.mockRejectedValueOnce(new Error("unsupported"));
    const result = await dispatchPendingContinuation(app, snapshot);
    expect(sendMessage).not.toHaveBeenCalled();
    expect(callServerTool.mock.calls[1]?.[0].arguments).toMatchObject({
      action: "complete_continuation", input: { outcome: "paused" },
    });
    expect(result.continuation.state).toBe("paused");
  });

  it("uses the originating server descriptor name and submits only exact binding/action fields", async () => {
    const snapshotOutput = { content: [], structuredContent: { kind: "snapshot", snapshot: terminalSnapshot() } };
    const { app, callServerTool } = appMock([snapshotOutput, snapshotOutput, snapshotOutput, snapshotOutput, snapshotOutput]);
    await readCurrentResult(app, BINDING);
    await confirmResult(app, BINDING);
    await selectResult(app, BINDING, [{ question_id: "scope", option_id: "email" }]);
    await cancelResult(app, BINDING);
    await requestAgentEdit(app, BINDING, "把地区改成西班牙");
    expect(RESULT_STATE_TOOL_NAME).toBe("onecrew_result_state");
    expect(callServerTool.mock.calls.map(([request]) => request.arguments)).toEqual([
      { action: "read_current", binding: BINDING },
      { action: "confirm", binding: BINDING },
      { action: "select", binding: BINDING, answers: [{ question_id: "scope", option_id: "email" }] },
      { action: "cancel", binding: BINDING },
      { action: "request_edit", input: { binding: BINDING, instruction: "把地区改成西班牙" } },
    ]);
    expect(callServerTool).toHaveBeenLastCalledWith({
      name: "onecrew_result_state",
      arguments: { action: "request_edit", input: { binding: BINDING, instruction: "把地区改成西班牙" } },
    }, { timeout: 60_000 });
  });

  it("rejects untyped success and preserves exact server failure categories", async () => {
    const invalid = appMock([{ content: [], structuredContent: { status: "ok" } }]);
    await expect(callResultState(invalid.app, { action: "read_current", binding: BINDING })).rejects.toMatchObject({
      failure: { kind: "invalid_response", reason: "unrecognized_root" },
    });
    const expired = appMock([{ content: [], isError: true, structuredContent: { code: "expired" } }]);
    await expect(callResultState(expired.app, { action: "read_current", binding: BINDING })).rejects.toMatchObject({ failure: { kind: "server", failure: { code: "expired" } } });
  });

  it("returns the consumed confirmation snapshot and native diagnostic without replaying the action", async () => {
    const response = { kind: "snapshot", snapshot: dmFailedSnapshot(), dm_failure: DM_FAILURE };
    const { app, callServerTool } = appMock([
      { content: [], structuredContent: response },
      { content: [], isError: true, structuredContent: { code: "replay" } },
    ]);
    expect(await confirmResult(app, BINDING)).toEqual(response);
    expect(callServerTool).toHaveBeenCalledTimes(1);
    await expect(confirmResult(app, BINDING)).rejects.toMatchObject({ failure: { kind: "server", failure: { code: "replay" } } });
    expect(callServerTool.mock.calls.map(([request]) => request.arguments)).toEqual([
      { action: "confirm", binding: BINDING }, { action: "confirm", binding: BINDING },
    ]);
  });

  it("keeps ordinary confirmation responses free of diagnostic fields", async () => {
    const response = { kind: "snapshot", snapshot: terminalSnapshot() };
    const { app } = appMock([{ content: [], structuredContent: response }]);
    expect(await confirmResult(app, BINDING)).toEqual(response);
  });

  it("classifies invalid Host result shapes without exposing response values", () => {
    expect(classifyInvalidResultResponse({})).toBe("missing_structured_content");
    expect(classifyInvalidResultResponse({ structuredContent: "snapshot" })).toBe("structured_content_not_object");
    expect(classifyInvalidResultResponse({ isError: true, structuredContent: {} })).toBe("error_without_typed_failure");
    expect(classifyInvalidResultResponse({ isError: true, structuredContent: { kind: "snapshot" } })).toBe("success_marked_error");
    expect(classifyInvalidResultResponse({ structuredContent: { kind: "snapshot" } })).toBe("snapshot_root_rejected");
    expect(classifyInvalidResultResponse({ structuredContent: { kind: "claimed_continuation" } })).toBe("claimed_contract_rejected");
    expect(classifyInvalidResultResponse({ structuredContent: { kind: "completed_continuation" } })).toBe("completed_contract_rejected");
    expect(classifyInvalidResultResponse({ structuredContent: { code: "expired", raw: "hidden" } })).toBe("failure_contract_rejected");
    expect(classifyInvalidResultResponse({ structuredContent: { status: "ok" } })).toBe("unrecognized_root");
  });

  it("claims, updates required text context, sends the server-owned message once, and acknowledges delivery in order", async () => {
    const snapshot = terminalSnapshot();
    const claimed = {
      binding: snapshot.binding,
      continuation_revision: 3,
      lease_id: `rcl_${"d".repeat(32)}`,
      request: { message: "已确认搜索计划，请按计划继续搜索。", requires_model_context: true, context: {
        kind: "search_plan_confirmed", confirmed_plan_ref: "12345678-1234-4234-8234-123456789abc", description: "只找土耳其语 RWA 创作者", keywords: ["RWA"] } },
      dispatch_started_at_ms: 10,
      dispatch_not_after_ms: 130,
    };
    const completed = {
      binding: { ...snapshot.binding, revision: 4 },
      continuation: { state: "delivered", continuation_revision: 4, dispatch_started_at_ms: null, dispatch_not_after_ms: null },
    };
    const { app, callServerTool, updateModelContext, sendMessage } = appMock([
      { content: [], structuredContent: { kind: "claimed_continuation", claimed } },
      { content: [], structuredContent: { kind: "completed_continuation", completed } },
    ]);
    const result = await dispatchPendingContinuation(app, snapshot);
    expect(callServerTool.mock.calls[0]?.[0].arguments).toEqual({
      action: "claim_continuation",
      input: { binding: snapshot.binding, expected_continuation_revision: 2, mode: "pending" },
    });
    expect(updateModelContext).toHaveBeenCalledExactlyOnceWith({ content: [
      { type: "text", text: expect.stringContaining(`result_ref: ${snapshot.binding.result_ref}`) },
    ] });
    const contextRequest = updateModelContext.mock.calls[0] as unknown as [{ content: [{ text: string }] }];
    expect(contextRequest[0].content[0].text).toContain('- description: "只找土耳其语 RWA 创作者"');
    expect(contextRequest[0].content[0].text).not.toContain(claimed.request.message);
    expect(contextRequest[0].content[0].text).not.toContain(claimed.lease_id);
    expect(sendMessage).toHaveBeenCalledWith({ role: "user", content: [{ type: "text", text: claimed.request.message }] });
    expect(sendMessage).toHaveBeenCalledTimes(1);
    expect(callServerTool.mock.invocationCallOrder[0]).toBeLessThan(updateModelContext.mock.invocationCallOrder[0]!);
    expect(updateModelContext.mock.invocationCallOrder[0]).toBeLessThan(sendMessage.mock.invocationCallOrder[0]!);
    expect(sendMessage.mock.invocationCallOrder[0]).toBeLessThan(callServerTool.mock.invocationCallOrder[1]!);
    expect(callServerTool.mock.calls[1]?.[0].arguments).toEqual({
      action: "complete_continuation", input: {
        binding: claimed.binding, expected_continuation_revision: 3,
        lease_id: claimed.lease_id, outcome: "delivered",
      },
    });
    const completionArgs = callServerTool.mock.calls[1]?.[0].arguments as { input?: { outcome?: string } } | undefined;
    expect(completionArgs?.input?.outcome).toBe("delivered");
    expect(result.continuation.state).toBe("delivered");
    await dispatchPendingContinuation(app, result);
    expect(sendMessage).toHaveBeenCalledTimes(1);
    expect(callServerTool).toHaveBeenCalledTimes(2);
  });

  it.each([
    ["edit_requested", "用户要求修改，原操作未执行。修改要求：\n\n把地区改成西班牙"],
    ["cancelled", "已取消，不执行此操作。"],
  ] as const)("sends a complete %s message verbatim without model context", async (kind, message) => {
    const snapshot = terminalSnapshot();
    const claimed = {
      binding: snapshot.binding, continuation_revision: 3, lease_id: `rcl_${"d".repeat(32)}`,
      dispatch_started_at_ms: 10, dispatch_not_after_ms: 130, request: { message, context: { kind } },
    };
    const { app, callServerTool, updateModelContext, sendMessage } = appMock([
      { content: [], structuredContent: { kind: "claimed_continuation", claimed } },
      { content: [], structuredContent: { kind: "completed_continuation", completed: {
        binding: snapshot.binding,
        continuation: { state: "delivered", continuation_revision: 4, dispatch_started_at_ms: null, dispatch_not_after_ms: null },
      } } },
    ]);
    const result = await dispatchPendingContinuation(app, snapshot);
    expect(updateModelContext).not.toHaveBeenCalled();
    expect(sendMessage).toHaveBeenCalledExactlyOnceWith({ role: "user", content: [{ type: "text", text: message }] });
    expect(callServerTool.mock.calls[1]?.[0].arguments).toMatchObject({ action: "complete_continuation", input: { outcome: "delivered" } });
    expect(result.continuation.state).toBe("delivered");
  });

  it.each(["unsupported", "rejected"] as const)("pauses required context when text is %s without a structured fallback", async (mode) => {
    const snapshot = terminalSnapshot();
    const claimed = {
      binding: snapshot.binding, continuation_revision: 3, lease_id: `rcl_${"d".repeat(32)}`,
      dispatch_started_at_ms: 10, dispatch_not_after_ms: 130,
      request: { message: "Continue approved search.", requires_model_context: true,
        context: { kind: "choice", answers: [{ question_id: "creator", option_id: "save" }] } },
    };
    const { app, callServerTool, updateModelContext, sendMessage, getHostCapabilities } = appMock([
      { content: [], structuredContent: { kind: "claimed_continuation", claimed } },
      { content: [], structuredContent: { kind: "completed_continuation", completed: {
        binding: snapshot.binding,
        continuation: { state: "paused", continuation_revision: 4, dispatch_started_at_ms: null, dispatch_not_after_ms: null },
      } } },
    ]);
    if (mode === "unsupported") getHostCapabilities.mockReturnValue({
      updateModelContext: { structuredContent: {} }, message: { text: {} },
    } as ReturnType<typeof getHostCapabilities>);
    else updateModelContext.mockRejectedValueOnce(new Error("Host rejected text context"));
    const onPause = vi.fn();
    const result = await dispatchPendingContinuation(app, snapshot, onPause);
    expect(result.continuation.state).toBe("paused");
    expect(onPause).toHaveBeenCalledExactlyOnceWith(mode === "unsupported" ? "text_unsupported" : "text_rejected");
    expect(updateModelContext).toHaveBeenCalledTimes(mode === "unsupported" ? 0 : 1);
    expect(sendMessage).not.toHaveBeenCalled();
    expect(callServerTool.mock.calls[1]?.[0].arguments).toMatchObject({ action: "complete_continuation", input: { outcome: "paused" } });
    await dispatchPendingContinuation(app, result, onPause);
    expect(callServerTool).toHaveBeenCalledTimes(2);
  });

  it("carries execution resource refs through the authoritative claim parser into text context", async () => {
    const snapshot = terminalSnapshot();
    const claimed = {
      binding: snapshot.binding, continuation_revision: 3, lease_id: `rcl_${"d".repeat(32)}`,
      dispatch_started_at_ms: 10, dispatch_not_after_ms: 130,
      request: { message: "The confirmed creator was saved.", requires_model_context: true, context: { kind: "execution", outcomes: [{
        call_id: "save-first", operation_id: "onecrew.creator.candidate.save", disposition: { kind: "succeeded" },
        summary: "Saved first creator", resource_refs: [{ kind: "creator", id: "creator-123", label: null }],
      }] } },
    };
    const { app, updateModelContext, sendMessage } = appMock([
      { content: [], structuredContent: { kind: "claimed_continuation", claimed } },
      { content: [], structuredContent: { kind: "completed_continuation", completed: {
        binding: snapshot.binding,
        continuation: { state: "delivered", continuation_revision: 4, dispatch_started_at_ms: null, dispatch_not_after_ms: null },
      } } },
    ]);
    const result = await dispatchPendingContinuation(app, snapshot);
    expect(result.continuation.state).toBe("delivered");
    expect(updateModelContext).toHaveBeenCalledExactlyOnceWith({ content: [{ type: "text", text: expect.stringContaining("creator-123") }] });
    expect(sendMessage).toHaveBeenCalledExactlyOnceWith({ role: "user", content: [{ type: "text", text: claimed.request.message }] });
  });

  it("acknowledges Host rejection as paused and transport ambiguity as delivery_unknown", async () => {
    const snapshot = terminalSnapshot();
    const claimed = {
      binding: snapshot.binding,
      continuation_revision: 3,
      lease_id: `rcl_${"d".repeat(32)}`,
      request: { message: "Continue safely.", context: { kind: "cancelled" } },
      dispatch_started_at_ms: 10,
      dispatch_not_after_ms: 130,
    };
    for (const [send, expected] of [
      [vi.fn(async () => ({ isError: true })), "paused"],
      [vi.fn(async () => { throw new Error("transport"); }), "delivery_unknown"],
    ] as const) {
      const { app, callServerTool } = appMock([
        { content: [], structuredContent: { kind: "claimed_continuation", claimed } },
        { content: [], structuredContent: { kind: "completed_continuation", completed: {
          binding: snapshot.binding,
          continuation: { state: expected, continuation_revision: 4, dispatch_started_at_ms: null, dispatch_not_after_ms: null },
        } } },
      ]);
      (app as unknown as { sendMessage: typeof send }).sendMessage = send;
      const onPause = vi.fn();
      const result = await dispatchPendingContinuation(app, snapshot, onPause);
      const completionArgs = callServerTool.mock.calls[1]?.[0].arguments as { input?: { outcome?: string } } | undefined;
      expect(completionArgs?.input?.outcome).toBe(expected);
      if (expected === "paused") expect(onPause).toHaveBeenCalledExactlyOnceWith("message_rejected");
      else expect(onPause).not.toHaveBeenCalled();
      await dispatchPendingContinuation(app, result, onPause);
      expect(send).toHaveBeenCalledTimes(1);
      expect(callServerTool).toHaveBeenCalledTimes(2);
    }
  });

  it("refreshes exactly at a dispatch lease boundary and never polls settled states", () => {
    const dispatching = {
      ...terminalSnapshot(),
      continuation: { state: "dispatching" as const, continuation_revision: 3, dispatch_started_at_ms: 1_000, dispatch_not_after_ms: 3_000 },
    };
    expect(continuationRefreshDelay(dispatching, 2_000)).toBe(1_050);
    expect(continuationRefreshDelay(dispatching, 4_000)).toBe(100);
    expect(continuationRefreshDelay(terminalSnapshot(), 2_000)).toBeNull();
    expect(resultRefreshDelay(dispatching, 2_000, 0)).toBe(1_050);
    expect(resultRefreshDelay(dispatching, 2_000, SUBMITTED_REFRESH_ATTEMPTS)).toBe(1_050);
  });

  // A submitted owner write runs inside the App's own request, so a lost
  // response leaves the card holding the last pre-terminal snapshot with no
  // lease to expire and nothing to push it. These pin the only escape.
  it("polls a submitted result on a backoff and then stops claiming progress", () => {
    const submitted = submittedSnapshot();
    const steps = [1_000, 2_000, 4_000, 8_000, 15_000];
    steps.forEach((step, attempt) => {
      expect(resultRefreshDelay(submitted, 0, attempt)).toBe(step);
    });
    for (const attempt of [steps.length, steps.length + 1, SUBMITTED_REFRESH_ATTEMPTS - 1]) {
      expect(resultRefreshDelay(submitted, 0, attempt)).toBe(30_000);
    }
    // Out of attempts is the card's signal to offer the state check instead
    // of an outcome it cannot confirm; it must never mean "settled".
    expect(resultRefreshDelay(submitted, 0, SUBMITTED_REFRESH_ATTEMPTS)).toBeNull();
    expect(resultRefreshDelay(submitted, 0, SUBMITTED_REFRESH_ATTEMPTS + 5)).toBeNull();
    // Total budget stays in minutes, not hours.
    const budgetMs = steps.reduce((sum, step) => sum + step, 0)
      + (SUBMITTED_REFRESH_ATTEMPTS - steps.length) * 30_000;
    expect(budgetMs).toBeGreaterThan(10 * 60_000);
    expect(budgetMs).toBeLessThan(30 * 60_000);
  });

  // The host's own 60s default expires while the server is still writing a
  // batch, and a response given up on that way is what stranded a card on
  // "processing" with five Creators already saved.
  it("budgets a confirmed action's rows instead of taking the host default", () => {
    const binding = BINDING;
    const batch = (rows: number) => resultStateTimeoutMs({
      action: "confirm_creator_save_batch", binding,
      selections: Array.from({ length: rows }, (_unused, index) => ({ row_id: `row-${index}`, confirmed_route_ids: ["route"] })),
    });
    expect(resultStateTimeoutMs({ action: "read_current", binding })).toBe(60_000);
    expect(resultStateTimeoutMs({ action: "cancel", binding })).toBe(60_000);
    expect(resultStateTimeoutMs({ action: "confirm", binding })).toBe(180_000);
    expect(batch(1)).toBe(180_000);
    expect(batch(5)).toBe(196_000);
    // Every write action outlives the host default it used to inherit.
    expect(batch(1)).toBeGreaterThan(60_000);
    // The advertised 50-row batch stays inside one bounded ceiling.
    expect(batch(50)).toBe(376_000);
    expect(batch(50)).toBeLessThan(900_000);
  });

  it("never polls a result the server has already settled", () => {
    for (const state of [
      { kind: "resolved" as const, code: "saved", summary: "已保存。" },
      { kind: "failed" as const, code: "creator_save_batch_partial", summary: "部分失败。", retryable: false },
      { kind: "unknown" as const, code: "creator_save_batch_unknown", summary: "结果未知。" },
      { kind: "available" as const },
    ]) {
      const settled = { ...terminalSnapshot(), state } as ResultSnapshot;
      expect(resultRefreshDelay(settled, 0, 0)).toBeNull();
    }
  });
});
