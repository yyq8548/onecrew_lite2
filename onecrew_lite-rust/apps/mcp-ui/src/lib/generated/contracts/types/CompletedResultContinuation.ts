/* Generated. Do not edit. */

export interface CompletedResultContinuation {
  binding: {
    fingerprint: string;
    origin: string;
    result_ref: string;
    revision: number;
    view: {
      identity: string;
      kind: "read_only" | "confirmation" | "choice";
    };
  };
  continuation: {
    continuation_revision: number;
    dispatch_not_after_ms: number | null;
    dispatch_started_at_ms: number | null;
    state: "not_requested" | "pending" | "dispatching" | "delivered" | "paused" | "delivery_unknown";
  };
}
