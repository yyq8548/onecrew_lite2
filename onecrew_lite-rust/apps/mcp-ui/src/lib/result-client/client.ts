import type { App } from "@modelcontextprotocol/ext-apps/react";
import type { CallToolRequest, CallToolResult } from "@modelcontextprotocol/sdk/types.js";

import {
  parseResultStateFailure,
  parseResultStateOutput,
  type ResultBinding,
  type CreatorSaveBatchSelection,
  type ResultChoiceAnswer,
  type ResultSnapshot,
  type ResultSnapshotOutput,
  type ResultStateInput,
  type ResultStateOutput,
  type SearchPlanPatch,
} from "@mcp-ui/lib/result-contract";
import { classifyInvalidResultResponse, resultResponseDiagnostics, ResultClientError } from "./errors";

export const RESULT_STATE_TOOL_NAME = "onecrew_result_state" as const;

/** Host request budget for an action that only reads or edits local state. */
const READ_TIMEOUT_MS = 60_000;
/** Admission plus the first owner write of a confirmed action. */
const WRITE_TIMEOUT_MS = 180_000;
/** Each further row of a batch runs inside the same one request. */
const WRITE_TIMEOUT_PER_ROW_MS = 4_000;
const WRITE_TIMEOUT_CEILING_MS = 900_000;

/** How long to let one App action run before calling it lost.
 *
 * A confirmed action performs its owner writes inside this single request,
 * row by row, after waiting for write admission that a concurrent sync can
 * hold. The host's own default is 60s, which a multi-row batch exceeds while
 * the server is still saving — and a response given up on that way strands
 * the card on "processing" and the Agent with no outcome. Budget the rows
 * instead; `resultRefreshDelay` still covers a response that is truly lost. */
export function resultStateTimeoutMs(input: ResultStateInput): number {
  const rows = input.action === "confirm_creator_save_batch" ? input.selections.length : 1;
  switch (input.action) {
    case "confirm":
    case "confirm_creator_save_batch":
    case "select":
      return Math.min(
        WRITE_TIMEOUT_CEILING_MS,
        WRITE_TIMEOUT_MS + Math.max(0, rows - 1) * WRITE_TIMEOUT_PER_ROW_MS,
      );
    default:
      return READ_TIMEOUT_MS;
  }
}

export async function callResultState(app: App, input: ResultStateInput): Promise<ResultStateOutput> {
  let result;
  try {
    result = await callResultStateTool(app, input);
  } catch {
    throw new ResultClientError({ kind: "transport" });
  }
  const output = parseResultStateOutput(result.structuredContent);
  if (output && result.isError !== true) return output;
  const failure = parseResultStateFailure(result.structuredContent);
  if (failure) throw new ResultClientError({ kind: "server", failure });
  const diagnostics = resultResponseDiagnostics(result);
  throw new ResultClientError({
    kind: "invalid_response",
    reason: classifyInvalidResultResponse(result),
    ...(diagnostics.length ? { diagnostics } : {}),
  });
}

export async function readCurrentResult(app: App, binding: ResultBinding): Promise<ResultSnapshot> {
  return requireSnapshot(await callResultState(app, { action: "read_current", binding }));
}

export async function confirmResult(app: App, binding: ResultBinding): Promise<ResultSnapshotOutput> {
  const output = await callResultState(app, { action: "confirm", binding });
  if (output.kind !== "snapshot") {
    throw new ResultClientError({ kind: "invalid_response", reason: "unrecognized_root" });
  }
  return output;
}

export async function confirmCreatorSaveBatch(app: App, binding: ResultBinding, selections: CreatorSaveBatchSelection[]): Promise<ResultSnapshot> {
  return requireSnapshot(await callResultState(app, { action: "confirm_creator_save_batch", binding, selections }));
}

export async function reviseSearchPlan(app: App, binding: ResultBinding, patch: SearchPlanPatch): Promise<ResultSnapshot> {
  const snapshot = requireSnapshot(await callResultState(app, { action: "revise_search_plan", binding, patch }));
  if (snapshot.binding.result_ref !== binding.result_ref || snapshot.binding.origin !== binding.origin ||
      snapshot.binding.fingerprint !== binding.fingerprint || snapshot.binding.view.identity !== binding.view.identity ||
      snapshot.binding.revision !== binding.revision + 1 || snapshot.state.kind !== "available" ||
      snapshot.presentation.kind !== "search_plan_editor" || snapshot.pending_action?.kind !== "search_plan") {
    throw new ResultClientError({ kind: "invalid_response", reason: "snapshot_consistency_rejected" });
  }
  return snapshot;
}

export async function selectResult(app: App, binding: ResultBinding, answers: ResultChoiceAnswer[]): Promise<ResultSnapshot> {
  return requireSnapshot(await callResultState(app, { action: "select", binding, answers }));
}

export async function cancelResult(app: App, binding: ResultBinding): Promise<ResultSnapshot> {
  return requireSnapshot(await callResultState(app, { action: "cancel", binding }));
}

export async function requestAgentEdit(app: App, binding: ResultBinding, instruction: string): Promise<ResultSnapshot> {
  return requireSnapshot(await callResultState(app, { action: "request_edit", input: { binding, instruction } }));
}

function requireSnapshot(output: ResultStateOutput): ResultSnapshot {
  if (output.kind !== "snapshot") {
    throw new ResultClientError({ kind: "invalid_response", reason: "unrecognized_root" });
  }
  return output.snapshot;
}

function callResultStateTool(app: App, input: ResultStateInput): Promise<CallToolResult> {
  // MCP Apps proxies this descriptor name to the App's originating server.
  // Namespacing belongs to the Host's model-facing surface, not App calls.
  return app.callServerTool(
    { name: RESULT_STATE_TOOL_NAME, arguments: input as CallToolRequest["params"]["arguments"] },
    { timeout: resultStateTimeoutMs(input) },
  );
}
