import { beforeEach, describe, expect, it, vi } from "vitest";
import { creatorInvoke } from "@/lib/creator";
import { confirmOutreach, prepareOutreach, readOutreachStatus, recordedOutreach, recordedOutreachStatus,
  type OutreachCompletion, type OutreachIntent, type OutreachRecordIds } from "./outreach-review";
import type { OwnerReviewSnapshot } from "./owner-review";
import type { DmFailure } from "./provider-error";

vi.mock("@/lib/creator", () => ({ creatorInvoke: vi.fn() }));
beforeEach(() => vi.mocked(creatorInvoke).mockReset());
const intent: OutreachIntent = { kind: "direct_message", input: {
  creator_id: "creator", contact_route_id: "route", resolved_target_id: "target", operator_account_ref_id: "operator",
  body: "Original message", allow_unfollowed_recipient: true, idempotency_key: "same-intent-key",
} };
const snapshot: OwnerReviewSnapshot = {
  binding: { result_ref: "rr_review", view: { identity: "rv_review", kind: "confirmation" }, fingerprint: "sha256:review", revision: 1, origin: "trusted-web-session" },
  completed_sections_count: 0,
  state: { kind: "available" }, pending_action: { kind: "confirm", prompt: "Send the original message?", label: "Confirm" },
  presentation: { title: "Message review", summary: null, sections: [] },
};
const recordIds: OutreachRecordIds = { kind: "direct_message", conversation_id: "conversation", message_id: "message", touchpoint_id: "touchpoint", audit_event_id: "audit" };
function completion(): OutreachCompletion {
  return { idempotency_key: intent.input.idempotency_key, intent_fingerprint: "sha256:v1:fixture", phase: "completed", creator_recorded: true, replayed: false,
    receipt: { kind: "message", external_message_id: "remote-message", external_thread_id: "remote-thread" },
    recorded: { kind: "direct_message", record: {
      conversation: { id: "conversation", account_ref_id: "operator", resolved_target_id: "target", channel: "x", provider_thread_id: "remote-thread", unread_count: 0, created_at: 1, updated_at: 1 },
      message: { id: "message", conversation_id: "conversation", external_message_id: "remote-message", direction: "outbound", body: "Original message", status: "sent", public_data: {}, sent_at: 1 },
      touchpoint: { id: "touchpoint", creator_id: "creator", kind: "direct_message", occurred_at: 1, metadata: {} },
      audit_event: { id: "audit", kind: "direct_message", payload: {}, created_at: 1 },
    } }, guidance: "Recorded",
  };
}

describe("typed Web outreach reviews", () => {
  it("preserves the terminal snapshot and native preflight failure from confirmation", async () => {
    const failure = { code: "direct_message_failed", message: "The read was unavailable.", failure: {
      stage: "message_capability", diagnostic: { platform: "telegram", protocol: "telegram.mtproto",
        origin: "provider" as const, code: "UNRECOGNIZED_RPC", http_status: null, rpc_code: 500,
        message: "The read was unavailable." },
    } };
    const terminal: OwnerReviewSnapshot = { ...snapshot, pending_action: null,
      state: { kind: "failed", code: "direct_message_failed", summary: "Preparation was rejected." } };
    vi.mocked(creatorInvoke).mockResolvedValueOnce({ snapshot: terminal, completion: null, failure });
    const result = await confirmOutreach(snapshot);
    expect(result.snapshot).toEqual(terminal);
    expect(result.failure).toEqual(failure);
    expect(result.completion).toBeNull();
  });

  it("never treats diagnostic wording or native codes as recording authority", () => {
    const value = completion();
    for (const message of ["sent successfully", "login required", "timeout"]) {
      const failure: DmFailure = { stage: "message_readback", diagnostic: {
        platform: "telegram", protocol: "telegram.mtproto", origin: "provider",
        code: "UNKNOWN_CODE", http_status: null, rpc_code: 500, message,
      } };
      const accepted = { ...value, phase: "remote_accepted" as const, failure };
      expect(recordedOutreach(accepted)).toBeNull();
      expect(accepted.receipt).toEqual(value.receipt);
      expect(recordedOutreach({ ...value, failure })).toBe(value.recorded);
    }
  });

  it("sends body and target only in preparation; confirmation cannot inject parameters or origin", async () => {
    vi.mocked(creatorInvoke).mockResolvedValueOnce(snapshot).mockResolvedValueOnce({ snapshot, completion: completion() });
    await prepareOutreach(intent);
    expect(creatorInvoke).toHaveBeenNthCalledWith(1, "onecrew_outreach_message_send_prepare", { input: intent.input });
    await confirmOutreach(snapshot);
    expect(creatorInvoke).toHaveBeenNthCalledWith(2, "onecrew_outreach_confirm", { input: {
      result_ref: snapshot.binding.result_ref, view: snapshot.binding.view, fingerprint: snapshot.binding.fingerprint, revision: 1,
    } });
    expect(intent.input).not.toHaveProperty("confirmed");
  });
  it("resumes the same immutable request after an uncertain confirmation response", async () => {
    const original = structuredClone(intent);
    vi.mocked(creatorInvoke).mockRejectedValueOnce(new Error("connection lost")).mockResolvedValueOnce(snapshot);
    await expect(confirmOutreach(snapshot)).rejects.toThrow("connection lost");
    await prepareOutreach(intent);
    expect(creatorInvoke).toHaveBeenLastCalledWith("onecrew_outreach_message_send_prepare", { input: original.input });
    expect(intent).toEqual(original);
    vi.mocked(creatorInvoke).mockResolvedValueOnce({ kind: "current", state: { phase: "record_pending", record: null } });
    await readOutreachStatus(intent);
    expect(creatorInvoke).toHaveBeenLastCalledWith("onecrew_outreach_action_status", { input: { idempotency_key: "same-intent-key" } });
  });
  it("does not treat acceptance, replay, or record-pending as a recorded success", () => {
    const value = completion();
    for (const phase of ["remote_effect_unknown", "remote_accepted", "record_pending", "rejected"] as const) {
      expect(recordedOutreach({ ...value, phase, replayed: true })).toBeNull();
    }
    expect(recordedOutreach({ ...value, creator_recorded: false })).toBeNull();
    expect(recordedOutreach({ ...value, recorded: null })).toBeNull();
    expect(recordedOutreach(value)).toBe(value.recorded);
  });
  it("accepts durable record IDs only with the completed current action", () => {
    for (const phase of ["remote_started", "remote_accepted", "record_pending", "refused"] as const) {
      expect(recordedOutreachStatus({ kind: "current", state: { phase, record: recordIds } })).toBeNull();
    }
    expect(recordedOutreachStatus({ kind: "retired", operation: "direct_message", phase: "completed", marked: false })).toBeNull();
    expect(recordedOutreachStatus({ kind: "current", state: { phase: "completed", record: recordIds } })).toBe(recordIds);
  });
});
