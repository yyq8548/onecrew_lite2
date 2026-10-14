import { creatorInvoke, type CommitMessageActionResult, type CommitRelationshipActionResult,
  type SendDirectMessageInput, type UpdateRelationshipInput, type Touchpoint, type AuditEvent } from "@/lib/creator";
import { webResultBinding, type OwnerReviewSnapshot } from "@/lib/owner-review";
import type { DmFailure, OperationFailure } from "@/lib/provider-error";

/** The original form intent is retained only to resume the same durable key. */
export type OutreachIntent =
  | { kind: "relationship"; input: Readonly<UpdateRelationshipInput> }
  | { kind: "direct_message"; input: Readonly<SendDirectMessageInput> };
export type CreatorOutreachRecord =
  | { kind: "relationship"; record: CommitRelationshipActionResult }
  | { kind: "direct_message"; record: CommitMessageActionResult }
  | { kind: "content_reply"; record: { external_reply_id: string; touchpoint: Touchpoint; audit_event: AuditEvent } };
export type OutreachCompletion = {
  idempotency_key: string;
  intent_fingerprint: string;
  phase: "remote_effect_unknown" | "remote_accepted" | "record_pending" | "completed" | "rejected";
  creator_recorded: boolean;
  replayed: boolean;
  receipt: null | { kind: "relationship"; target_external_id: string }
    | { kind: "content_reply"; external_reply_id: string }
    | { kind: "message"; external_message_id: string; external_thread_id: string | null };
  recorded: CreatorOutreachRecord | null;
  guidance: string;
  /** This diagnostic does not determine the phase or permission to resend. */
  failure?: DmFailure;
};
export type OutreachRecordIds =
  | { kind: "relationship"; relationship_id: string; can_message_id: string; touchpoint_id: string; audit_event_id: string }
  | { kind: "content_reply"; external_reply_id: string; touchpoint_id: string; audit_event_id: string }
  | { kind: "direct_message"; conversation_id: string; message_id: string; touchpoint_id: string; audit_event_id: string };
/** Only body-free status fields consumed by this Web view. */
export type OutreachStatus =
  | { kind: "current"; state: { phase: "remote_started" | "remote_accepted" | "record_pending" | "completed" | "refused"; record: OutreachRecordIds | null } }
  | { kind: "retired"; operation: string; phase: "completed" | "failed" | "retired_action_unresolved"; marked: boolean };
export type WebOutreachConfirmationResult = {
  snapshot: OwnerReviewSnapshot;
  completion: OutreachCompletion | null;
  /** A fresh preflight may fail after the review has already been consumed. */
  failure?: OperationFailure;
};
export type OutreachReviewState = { intent: OutreachIntent; snapshot: OwnerReviewSnapshot; confirmationAttempted: boolean };

export function prepareOutreach(intent: OutreachIntent): Promise<OwnerReviewSnapshot> {
  if (intent.kind === "relationship") {
    return creatorInvoke("onecrew_relationship_update_prepare", { input: intent.input });
  }
  return creatorInvoke("onecrew_outreach_message_send_prepare", { input: intent.input });
}
export function confirmOutreach(snapshot: OwnerReviewSnapshot) {
  return creatorInvoke<WebOutreachConfirmationResult>("onecrew_outreach_confirm", { input: webResultBinding(snapshot.binding) });
}
export function readOutreachResult(snapshot: OwnerReviewSnapshot) {
  return creatorInvoke<OwnerReviewSnapshot>("onecrew_outreach_result_get", { input: webResultBinding(snapshot.binding) });
}
export function cancelOutreach(snapshot: OwnerReviewSnapshot) {
  return creatorInvoke<OwnerReviewSnapshot>("onecrew_outreach_cancel", { input: webResultBinding(snapshot.binding) });
}
export function readOutreachStatus(intent: OutreachIntent) {
  return creatorInvoke<OutreachStatus>("onecrew_outreach_action_status", { input: { idempotency_key: intent.input.idempotency_key } });
}
export function recordedOutreach(completion: OutreachCompletion | null): CreatorOutreachRecord | null {
  return completion?.phase === "completed" && completion.creator_recorded ? completion.recorded : null;
}
export function recordedOutreachStatus(status: OutreachStatus): OutreachRecordIds | null {
  return status.kind === "current" && status.state.phase === "completed" ? status.state.record : null;
}
