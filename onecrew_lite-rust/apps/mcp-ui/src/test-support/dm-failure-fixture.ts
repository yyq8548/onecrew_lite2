import type { ResultDmFailure, ResultSnapshot } from "@mcp-ui/lib/result-contract";
import { SNAPSHOT } from "@mcp-ui/test-support/fixtures";

export const DM_FAILURE: ResultDmFailure = {
  stage: "message_capability",
  diagnostic: {
    platform: "telegram",
    protocol: "mtproto",
    origin: "provider",
    code: "FLOOD_WAIT_17",
    http_status: null,
    rpc_code: 420,
    message: "The provider could not complete the capability check.",
  },
};

export function dmReviewSnapshot(): ResultSnapshot {
  return {
    ...structuredClone(SNAPSHOT),
    locale: "en-US",
    presentation: {
      kind: "outreach_confirmation",
      title: "Review message",
      summary: null,
      sections: [{ kind: "fields", heading: null, fields: [{ label: "Body", value: "The exact reviewed message" }] }],
    },
    pending_action: { kind: "confirm", prompt: "Send this exact message?", label: "Confirm message" },
    batch: { outcomes: [{
      call_id: "call.message", operation_id: "onecrew.outreach.message.send",
      disposition: { kind: "not_run", reason: "confirmation_required" },
      summary: "Message requires confirmation.", resource_refs: [],
    }] },
  };
}

export function dmFailedSnapshot(): ResultSnapshot {
  const review = dmReviewSnapshot();
  review.binding.revision++; review.binding.revision++;
  return {
    ...review,
    binding: { ...review.binding },
    pending_action: null,
    completed_sections_count: 1,
    state: { kind: "failed", code: "outreach_preflight_failed", summary: "The confirmed message could not proceed.", retryable: false },
    presentation: {
      kind: "outreach_outcome", title: "Message outcome", summary: null,
      sections: [{ kind: "notice", heading: null, tone: "neutral", body: "Earlier completed work is retained." }],
    },
    batch: { outcomes: [{ ...review.batch.outcomes[0]!, disposition: { kind: "failed", code: "outreach_preflight_failed" } }] },
  };
}
