import type { App } from "@modelcontextprotocol/ext-apps/react";

import {
  type ClaimedResultContinuation,
  type ResultSnapshot,
} from "@mcp-ui/lib/result-contract";
import { callResultState } from "./client";
import { ResultClientError } from "./errors";
import { formatContinuationText } from "@mcp-ui/lib/result-client/continuation-text";

export type ContinuationPauseReason = "text_unsupported" | "text_rejected" | "message_rejected";

export async function dispatchPendingContinuation(app: App, snapshot: ResultSnapshot, onPause?: (reason: ContinuationPauseReason) => void): Promise<ResultSnapshot> {
  if (snapshot.continuation.state !== "pending") return snapshot;
  const claimedOutput = await callResultState(app, {
    action: "claim_continuation",
    input: {
      binding: snapshot.binding,
      expected_continuation_revision: snapshot.continuation.continuation_revision,
      mode: "pending",
    },
  });
  if (claimedOutput.kind !== "claimed_continuation") {
    throw new ResultClientError({ kind: "invalid_response", reason: "unrecognized_root" });
  }
  const claimed = claimedOutput.claimed;
  // Hosts may show model context to the user, so it is sent only when it
  // carries content the server-authored message omits; then it must be
  // accepted before that message is sent verbatim.
  const contextResult = claimed.request.requires_model_context
    ? await updateSafeModelContext(app, claimed, snapshot.locale)
    : null;
  const missingRequiredContext = contextResult !== null && contextResult !== "delivered";
  const outcome = missingRequiredContext
    ? "paused"
    : await sendContinuation(app, claimed);
  const completed = await callResultState(app, {
    action: "complete_continuation",
    input: {
      binding: claimed.binding,
      expected_continuation_revision: claimed.continuation_revision,
      lease_id: claimed.lease_id,
      outcome,
    },
  });
  if (completed.kind !== "completed_continuation") {
    throw new ResultClientError({ kind: "invalid_response", reason: "unrecognized_root" });
  }
  if (outcome === "paused") onPause?.(missingRequiredContext ? contextResult === "unsupported" ? "text_unsupported" : "text_rejected" : "message_rejected");
  return {
    ...snapshot,
    binding: completed.completed.binding,
    continuation: completed.completed.continuation,
  };
}

export function continuationRefreshDelay(snapshot: ResultSnapshot, nowMs: number): number | null {
  if (snapshot.continuation.state !== "dispatching" || snapshot.continuation.dispatch_not_after_ms == null) return null;
  const delay = BigInt(snapshot.continuation.dispatch_not_after_ms) - BigInt(Math.floor(nowMs)) + 50n;
  if (delay < 100n) return 100;
  return delay > 2_147_483_647n ? 2_147_483_647 : Number(delay);
}

const SUBMITTED_REFRESH_STEPS_MS = [1_000, 2_000, 4_000, 8_000, 15_000];
const SUBMITTED_REFRESH_STEADY_MS = 30_000;
/** Attempts a submitted card polls before it stops claiming progress. */
export const SUBMITTED_REFRESH_ATTEMPTS = 40;

/** How long to wait before re-reading a result the server has not settled.
 *
 * A dispatching continuation refreshes exactly at its lease boundary. A
 * `submitted` result has neither a lease nor a push channel, and the owner
 * write runs inside the App's own request: when that response is lost the
 * card is left holding the only pre-terminal snapshot anyone will hand it.
 * Poll instead, because the terminal transition always lands — the owner
 * commits one even when its dispatch is dropped — and return null after the
 * last attempt so the card stops claiming progress it cannot confirm. Every
 * settled state returns null on the first call: a finished result is never
 * polled. */
export function resultRefreshDelay(
  snapshot: ResultSnapshot,
  nowMs: number,
  submittedAttempts: number,
): number | null {
  const lease = continuationRefreshDelay(snapshot, nowMs);
  if (lease != null) return lease;
  if (snapshot.state.kind !== "submitted") return null;
  if (submittedAttempts >= SUBMITTED_REFRESH_ATTEMPTS) return null;
  return SUBMITTED_REFRESH_STEPS_MS[submittedAttempts] ?? SUBMITTED_REFRESH_STEADY_MS;
}

async function updateSafeModelContext(app: App, claimed: ClaimedResultContinuation, locale: ResultSnapshot["locale"]): Promise<"delivered" | "unsupported" | "rejected"> {
  if (app.getHostCapabilities()?.updateModelContext?.text == null) return "unsupported";
  try {
    await app.updateModelContext({ content: [{ type: "text", text: formatContinuationText(claimed, locale) }] });
    return "delivered";
  } catch { return "rejected"; }
}

async function sendContinuation(
  app: App,
  claimed: ClaimedResultContinuation,
): Promise<"delivered" | "paused" | "delivery_unknown"> {
  try {
    const sent = await app.sendMessage({
      role: "user",
      content: [{ type: "text", text: claimed.request.message }],
    });
    return sent.isError ? "paused" : "delivered";
  } catch {
    return "delivery_unknown";
  }
}
