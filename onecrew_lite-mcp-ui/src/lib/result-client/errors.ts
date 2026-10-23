import { diagnoseResultSnapshot, resultSnapshotDiagnostics, type ResultStateFailure, type ResultSchemaDiagnostic } from "@mcp-ui/lib/result-contract";

export type ResultClientFailure =
  | { kind: "server"; failure: ResultStateFailure }
  | { kind: "invalid_response"; reason: InvalidResultResponseReason; diagnostics?: ResultSchemaDiagnostic[] }
  | { kind: "transport" };

export type InvalidResultResponseReason =
  | "missing_structured_content"
  | "structured_content_not_object"
  | "error_without_typed_failure"
  | "success_marked_error"
  | "snapshot_root_rejected"
  | "snapshot_binding_rejected"
  | "snapshot_batch_rejected"
  | "snapshot_presentation_rejected"
  | "snapshot_pending_action_rejected"
  | "snapshot_state_rejected"
  | "snapshot_continuation_rejected"
  | "snapshot_consistency_rejected"
  | "claimed_contract_rejected"
  | "completed_contract_rejected"
  | "failure_contract_rejected"
  | "unrecognized_root";

export class ResultClientError extends Error {
  constructor(readonly failure: ResultClientFailure) {
    super(failure.kind === "server" ? failure.failure.code : failure.kind);
    this.name = "ResultClientError";
  }
}

export function resultResponseDiagnostics(result: { structuredContent?: unknown }): ResultSchemaDiagnostic[] {
  const item = result.structuredContent;
  if (!item || typeof item !== "object" || Array.isArray(item)) return [];
  const root = item as Record<string, unknown>;
  return root.kind === "snapshot" ? resultSnapshotDiagnostics(root.snapshot) : [];
}

export function classifyInvalidResultResponse(result: {
  isError?: boolean;
  structuredContent?: unknown;
}): InvalidResultResponseReason {
  const value = result.structuredContent;
  if (value == null) return "missing_structured_content";
  if (typeof value !== "object" || Array.isArray(value)) return "structured_content_not_object";
  const item = value as Record<string, unknown>;
  if (result.isError === true) {
    if (typeof item.code === "string") return "failure_contract_rejected";
    if (typeof item.kind === "string") return "success_marked_error";
    return "error_without_typed_failure";
  }
  if (item.kind === "snapshot") {
    const reason = diagnoseResultSnapshot(item.snapshot) ?? "root";
    return `snapshot_${reason}_rejected`;
  }
  if (item.kind === "claimed_continuation") return "claimed_contract_rejected";
  if (item.kind === "completed_continuation") return "completed_contract_rejected";
  if (typeof item.code === "string") return "failure_contract_rejected";
  return "unrecognized_root";
}
